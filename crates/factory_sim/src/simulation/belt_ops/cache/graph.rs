use super::{PATCH_STORAGE_HEADROOM, TransportDirtyRegion};
use crate::simulation::belt_ops::geometry::{belt_downstream_lane_key, splitter_output_lane_key};
use crate::simulation::belt_ops::types::{
    TRANSPORT_LANE_SLOTS_PER_ENTITY, TransportLaneDownstream, TransportLaneIndex, TransportLaneKey,
    TransportRunIndex, lane_raw_index,
};
use crate::simulation::{EntityId, EntityStore, SmallVec, WorldTileCoord};

const VACANT_SLOT: u32 = u32::MAX;

/// Hot derived state for one dense transport-lane slot.
///
/// Keeping key, routing, run membership, and speed together makes traversal
/// consume one compact record instead of chasing parallel vectors. `key` is
/// `None` only for a slot retained on the incremental patch free list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) struct TransportLaneRecord {
    pub(in crate::simulation::belt_ops) key: Option<TransportLaneKey>,
    pub(in crate::simulation::belt_ops) downstream: TransportLaneDownstream,
    run: u32,
    run_position: u32,
    pub(in crate::simulation::belt_ops) speed_subtiles_per_tick: u16,
}

impl TransportLaneRecord {
    fn occupied(key: TransportLaneKey, speed_subtiles_per_tick: u16) -> Self {
        Self {
            key: Some(key),
            downstream: TransportLaneDownstream::Missing,
            run: VACANT_SLOT,
            run_position: VACANT_SLOT,
            speed_subtiles_per_tick,
        }
    }

    fn free(&mut self) {
        self.key = None;
        self.downstream = TransportLaneDownstream::Missing;
        self.run = VACANT_SLOT;
        self.run_position = VACANT_SLOT;
        self.speed_subtiles_per_tick = 0;
    }
}

/// One maximal chain of belt lanes advanced as a unit. `start..start + len`
/// indexes [`TransportLaneGraph::run_lane_slots`] in upstream-to-downstream
/// order. `cyclic` marks pure loops whose tail feeds the run's own head; the
/// tail's carry is blocked there because the head advances last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TransportRunRecord {
    start: u32,
    len: u32,
    cyclic: bool,
}

/// Adjacency index over transport lanes using compact slots: every existing
/// belt/splitter lane gets a dense slot id at rebuild time, so the per-lane
/// arrays the advancement loop walks stay proportional to the lane count
/// instead of the peak entity id. The sparse `slot_by_raw` indirection maps
/// `entity_id * 4 + lane_offset` wakeup keys onto slots.
///
/// On top of the lane adjacency, lanes are grouped into runs (see
/// [`TransportRunRecord`]): scheduling, visit states, and activity tracking
/// operate on runs, while item movement still reads per-lane state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneGraph {
    slot_by_raw: Vec<u32>,
    lanes: Vec<TransportLaneRecord>,
    upstream_by_slot: Vec<SmallVec<[TransportLaneIndex; 2]>>,
    run_lane_slots: Vec<TransportLaneIndex>,
    run_records: Vec<TransportRunRecord>,
    /// Slots of removed entities, reusable by later placements. Slot arrays
    /// are only compacted by a full rebuild.
    free_slots: Vec<u32>,
    /// Largest underground reach seen among placed entities; bounds how far a
    /// tile edit can affect lane resolution.
    max_underground_distance: u8,
}

impl TransportLaneGraph {
    pub(super) fn rebuild(&mut self, entities: &EntityStore) {
        let raw_len = transport_lane_index_len(entities);
        let lane_count = entities
            .transport_belts
            .len()
            .saturating_mul(2)
            .saturating_add(entities.splitters.len().saturating_mul(4));
        self.slot_by_raw.clear();
        self.slot_by_raw
            .reserve(raw_len.saturating_add(PATCH_STORAGE_HEADROOM));
        self.slot_by_raw.resize(raw_len, VACANT_SLOT);
        self.lanes.clear();
        self.lanes
            .reserve(lane_count.saturating_add(PATCH_STORAGE_HEADROOM));
        self.upstream_by_slot.clear();
        self.upstream_by_slot
            .reserve(lane_count.saturating_add(PATCH_STORAGE_HEADROOM));

        // Pass 1: assign compact slots in deterministic entity-id order so
        // save/load reproduces the same slot layout.
        for &entity_id in entities.transport_belts.keys() {
            let speed_subtiles_per_tick = entities
                .transport_belts
                .get(&entity_id)
                .expect("iterated transport belt should exist")
                .speed_subtiles_per_tick;
            for lane_index in 0..2 {
                self.assign_slot(
                    TransportLaneKey::Belt {
                        entity_id,
                        lane_index,
                    },
                    speed_subtiles_per_tick,
                );
            }
        }
        for &entity_id in entities.splitters.keys() {
            let speed_subtiles_per_tick = entities
                .splitters
                .get(&entity_id)
                .expect("iterated splitter should exist")
                .speed_subtiles_per_tick;
            for input_port in 0..2 {
                for lane_index in 0..2 {
                    self.assign_slot(
                        TransportLaneKey::Splitter {
                            entity_id,
                            input_port,
                            lane_index,
                        },
                        speed_subtiles_per_tick,
                    );
                }
            }
        }

        // Pass 2: resolve adjacency now that every lane has a slot.
        self.upstream_by_slot
            .resize_with(self.lanes.len(), SmallVec::new);
        for slot in 0..self.lanes.len() {
            let index = TransportLaneIndex::from_slot(slot);
            let downstream = self.resolve_downstream(entities, slot);
            self.lanes[slot].downstream = downstream;
            for target in downstream_targets(downstream) {
                self.push_upstream(target, index);
            }
        }

        self.free_slots.clear();
        self.max_underground_distance = entities
            .transport_belts
            .values()
            .filter_map(|segment| segment.underground)
            .map(|underground| underground.max_distance)
            .max()
            .unwrap_or(0);
        self.rebuild_runs();
    }

    fn resolve_downstream(&self, entities: &EntityStore, slot: usize) -> TransportLaneDownstream {
        let Some(key) = self.lanes.get(slot).and_then(|lane| lane.key) else {
            return TransportLaneDownstream::Missing;
        };
        match key {
            TransportLaneKey::Belt {
                entity_id,
                lane_index,
            } => TransportLaneDownstream::Belt {
                downstream: belt_downstream_lane_key(entities, entity_id, lane_index)
                    .and_then(|key| self.slot_for(key)),
            },
            TransportLaneKey::Splitter {
                entity_id,
                lane_index,
                ..
            } => TransportLaneDownstream::Splitter {
                outputs: [0, 1].map(|output_port| {
                    splitter_output_lane_key(entities, entity_id, output_port, lane_index)
                        .and_then(|key| self.slot_for(key))
                }),
            },
        }
    }

    /// Applies scoped topology edits without touching untouched lanes. Returns
    /// the ids of runs created by the patch, or `None` when the patch should
    /// fall back to a full rebuild (storage compaction due).
    pub(super) fn patch(
        &mut self,
        entities: &EntityStore,
        regions: &[TransportDirtyRegion],
        catalog_underground_distance: u8,
    ) -> Option<std::ops::Range<usize>> {
        if self.run_records.len() > 2 * self.lanes.len() + 1024
            || self.run_lane_slots.len() > 2 * self.lanes.len() + 1024
        {
            return None;
        }

        let raw_len = transport_lane_index_len(entities);
        if self.slot_by_raw.len() < raw_len {
            self.slot_by_raw.resize(raw_len, VACANT_SLOT);
        }
        let reach = i64::from(catalog_underground_distance.max(self.max_underground_distance)) + 1;

        let mut affected_entities = Vec::with_capacity(regions.len().saturating_mul(64));
        let mut dissolved_runs = Vec::with_capacity(regions.len().saturating_mul(8));
        let mut candidates: Vec<usize> = Vec::new();

        for region in regions {
            if entities.transport_belts.contains_key(&region.entity_id)
                || entities.splitters.contains_key(&region.entity_id)
            {
                affected_entities.push(region.entity_id);
            } else {
                self.free_entity_slots(region.entity_id, &mut dissolved_runs);
            }
            for (x, y) in region.footprint.tiles() {
                self.collect_affected_around(entities, x, y, reach, &mut affected_entities);
            }
        }

        affected_entities.sort_unstable();
        affected_entities.dedup();
        for &entity_id in &affected_entities {
            self.ensure_entity_slots(entities, entity_id, &mut candidates);
        }

        for &entity_id in &affected_entities {
            for slot in self.entity_slot_list(entity_id) {
                dissolved_runs.extend(self.run_id_at(slot));
                let old = self.lanes[slot].downstream;
                let new = self.resolve_downstream(entities, slot);
                if old == new {
                    continue;
                }
                self.detach_edges(slot, old, &mut dissolved_runs);
                self.lanes[slot].downstream = new;
                self.attach_edges(
                    TransportLaneIndex::from_slot(slot),
                    new,
                    &mut dissolved_runs,
                );
            }
        }

        dissolved_runs.sort_unstable();
        dissolved_runs.dedup();
        for &run in &dissolved_runs {
            let record = self.run_records[run as usize];
            self.run_records[run as usize].len = 0;
            let start = record.start as usize;
            for i in start..start + record.len as usize {
                let slot = self.run_lane_slots[i].raw();
                if self.lanes[slot].run == run {
                    self.lanes[slot].run = VACANT_SLOT;
                    self.lanes[slot].run_position = VACANT_SLOT;
                    if self.lanes[slot].key.is_some() {
                        candidates.push(slot);
                    }
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();

        let first_new_run = self.run_records.len();
        for &slot in &candidates {
            if self.lanes[slot].key.is_some()
                && self.lanes[slot].run == VACANT_SLOT
                && self.is_run_head(slot)
            {
                self.build_run_from(slot);
            }
        }
        for &slot in &candidates {
            if self.lanes[slot].key.is_some() && self.lanes[slot].run == VACANT_SLOT {
                self.build_run_from(slot);
            }
        }
        Some(first_new_run..self.run_records.len())
    }

    /// Transport entities whose downstream resolution can observe tile
    /// `(x, y)`: the occupant, direct neighbors, and underground endpoints
    /// whose pairing scan crosses the tile.
    fn collect_affected_around(
        &self,
        entities: &EntityStore,
        x: WorldTileCoord,
        y: WorldTileCoord,
        reach: i64,
        affected: &mut Vec<EntityId>,
    ) {
        let add = |x: WorldTileCoord, y: WorldTileCoord, affected: &mut Vec<EntityId>| {
            if let Some(entity_id) = entities.occupancy.entity_at(x, y)
                && (entities.transport_belts.contains_key(&entity_id)
                    || entities.splitters.contains_key(&entity_id))
            {
                affected.push(entity_id);
            }
        };
        add(x, y, affected);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            for offset in 1..=reach {
                add(x + dx * offset, y + dy * offset, affected);
            }
        }
    }

    fn free_entity_slots(&mut self, entity_id: EntityId, dissolved_runs: &mut Vec<u32>) {
        let Ok(entity_index) = usize::try_from(entity_id.raw()) else {
            return;
        };
        let Some(base) = entity_index.checked_mul(TRANSPORT_LANE_SLOTS_PER_ENTITY) else {
            return;
        };
        for offset in 0..TRANSPORT_LANE_SLOTS_PER_ENTITY {
            let raw = base + offset;
            let Some(&slot) = self.slot_by_raw.get(raw) else {
                continue;
            };
            if slot == VACANT_SLOT {
                continue;
            }
            let slot_index = slot as usize;
            self.slot_by_raw[raw] = VACANT_SLOT;
            dissolved_runs.extend(self.run_id_at(slot_index));
            // Runs of feeders can merge across the removed lane, so they must
            // re-derive even though their own geometry is intact.
            let upstreams = std::mem::take(&mut self.upstream_by_slot[slot_index]);
            for upstream in upstreams {
                dissolved_runs.extend(self.run_id_at(upstream.raw()));
            }
            let old = self.lanes[slot_index].downstream;
            self.detach_edges(slot_index, old, dissolved_runs);
            self.lanes[slot_index].free();
            self.free_slots.push(slot);
        }
    }

    /// Allocates slots for entities placed since the last refresh, reusing
    /// freed slots where possible.
    fn ensure_entity_slots(
        &mut self,
        entities: &EntityStore,
        entity_id: EntityId,
        new_slots: &mut Vec<usize>,
    ) {
        let mut keys: SmallVec<[TransportLaneKey; 4]> = SmallVec::new();
        let speed_subtiles_per_tick;
        if let Some(segment) = entities.transport_belts.get(&entity_id) {
            speed_subtiles_per_tick = segment.speed_subtiles_per_tick;
            if let Some(underground) = segment.underground {
                self.max_underground_distance =
                    self.max_underground_distance.max(underground.max_distance);
            }
            for lane_index in 0..2 {
                keys.push(TransportLaneKey::Belt {
                    entity_id,
                    lane_index,
                });
            }
        } else if let Some(splitter) = entities.splitters.get(&entity_id) {
            speed_subtiles_per_tick = splitter.speed_subtiles_per_tick;
            for input_port in 0..2 {
                for lane_index in 0..2 {
                    keys.push(TransportLaneKey::Splitter {
                        entity_id,
                        input_port,
                        lane_index,
                    });
                }
            }
        } else {
            return;
        }

        for key in keys {
            let Some(raw) = lane_raw_index(key) else {
                continue;
            };
            if raw >= self.slot_by_raw.len() {
                self.slot_by_raw.resize(raw + 1, VACANT_SLOT);
            }
            if self.slot_by_raw[raw] != VACANT_SLOT {
                continue;
            }
            let slot = if let Some(free) = self.free_slots.pop() {
                let slot = free as usize;
                self.lanes[slot] = TransportLaneRecord::occupied(key, speed_subtiles_per_tick);
                self.upstream_by_slot[slot].clear();
                slot
            } else {
                let slot = self.lanes.len();
                let _ = u32::try_from(slot).expect("transport lane slot capacity exceeded");
                self.lanes
                    .push(TransportLaneRecord::occupied(key, speed_subtiles_per_tick));
                self.upstream_by_slot.push(SmallVec::new());
                slot
            };
            self.slot_by_raw[raw] = slot as u32;
            new_slots.push(slot);
        }
    }

    fn entity_slot_list(&self, entity_id: EntityId) -> SmallVec<[usize; 4]> {
        let mut slots = SmallVec::new();
        let Ok(entity_index) = usize::try_from(entity_id.raw()) else {
            return slots;
        };
        let Some(base) = entity_index.checked_mul(TRANSPORT_LANE_SLOTS_PER_ENTITY) else {
            return slots;
        };
        for offset in 0..TRANSPORT_LANE_SLOTS_PER_ENTITY {
            if let Some(&slot) = self.slot_by_raw.get(base + offset)
                && slot != VACANT_SLOT
            {
                slots.push(slot as usize);
            }
        }
        slots
    }

    fn detach_edges(
        &mut self,
        slot: usize,
        old: TransportLaneDownstream,
        dissolved_runs: &mut Vec<u32>,
    ) {
        let index = TransportLaneIndex::from_slot(slot);
        for target in downstream_targets(old) {
            dissolved_runs.extend(self.run_id_at(target.raw()));
            let Some(upstreams) = self.upstream_by_slot.get_mut(target.raw()) else {
                continue;
            };
            upstreams.retain(|upstream| *upstream != index);
            // The remaining feeder may become the target's single upstream,
            // merging their runs.
            for upstream in upstreams.clone() {
                dissolved_runs.extend(self.run_id_at(upstream.raw()));
            }
        }
    }

    fn attach_edges(
        &mut self,
        index: TransportLaneIndex,
        new: TransportLaneDownstream,
        dissolved_runs: &mut Vec<u32>,
    ) {
        for target in downstream_targets(new) {
            dissolved_runs.extend(self.run_id_at(target.raw()));
            // Existing feeders can lose chain-link status when the target
            // gains an upstream.
            if let Some(upstreams) = self.upstream_by_slot.get(target.raw()) {
                for upstream in upstreams.clone() {
                    dissolved_runs.extend(self.run_id_at(upstream.raw()));
                }
            }
            self.push_upstream(target, index);
        }
    }

    fn run_id_at(&self, slot: usize) -> Option<u32> {
        self.lanes
            .get(slot)
            .map(|lane| lane.run)
            .filter(|run| *run != VACANT_SLOT)
    }

    /// Groups lanes into maximal chains. A lane extends its predecessor's run
    /// exactly when it is the single continuation of a single belt upstream;
    /// splitter lanes, sideload merge targets, and splitter-fed lanes start
    /// new runs.
    fn rebuild_runs(&mut self) {
        let slot_count = self.lanes.len();
        for lane in &mut self.lanes {
            lane.run = VACANT_SLOT;
            lane.run_position = VACANT_SLOT;
        }
        self.run_lane_slots.clear();
        let lane_count = self.lanes.iter().filter(|lane| lane.key.is_some()).count();
        self.run_lane_slots.reserve(
            lane_count
                .saturating_mul(2)
                .saturating_add(PATCH_STORAGE_HEADROOM),
        );
        self.run_records.clear();

        // Pass 1: walk every chain from its head, in slot order so the run
        // layout is deterministic across save/load.
        for slot in 0..slot_count {
            if self.lanes[slot].key.is_some()
                && self.lanes[slot].run == VACANT_SLOT
                && self.is_run_head(slot)
            {
                self.build_run_from(slot);
            }
        }
        // Pass 2: lanes still unassigned sit on pure cycles where every lane
        // has a chain predecessor; break each cycle at its lowest slot.
        for slot in 0..slot_count {
            if self.lanes[slot].key.is_some() && self.lanes[slot].run == VACANT_SLOT {
                self.build_run_from(slot);
            }
        }
        self.run_records.reserve(PATCH_STORAGE_HEADROOM);
    }

    fn build_run_from(&mut self, head: usize) {
        let run = u32::try_from(self.run_records.len()).expect("transport run capacity exceeded");
        let start = self.run_lane_slots.len();
        let mut cyclic = false;
        let mut slot = head;
        loop {
            self.lanes[slot].run = run;
            self.lanes[slot].run_position = u32::try_from(self.run_lane_slots.len() - start)
                .expect("transport run position capacity exceeded");
            self.run_lane_slots
                .push(TransportLaneIndex::from_slot(slot));
            let Some(next) = self.chain_successor(slot) else {
                break;
            };
            if next == head {
                cyclic = true;
                break;
            }
            if self.lanes[next].run != VACANT_SLOT {
                break;
            }
            slot = next;
        }
        self.run_records.push(TransportRunRecord {
            start: u32::try_from(start).expect("transport run lane capacity exceeded"),
            len: u32::try_from(self.run_lane_slots.len() - start)
                .expect("transport run length capacity exceeded"),
            cyclic,
        });
    }

    /// The lane that continues `slot`'s chain: its single belt-to-belt
    /// downstream, provided that downstream is fed by `slot` alone.
    fn chain_successor(&self, slot: usize) -> Option<usize> {
        let lane = self.lanes.get(slot)?;
        if !matches!(lane.key, Some(TransportLaneKey::Belt { .. })) {
            return None;
        }
        let TransportLaneDownstream::Belt {
            downstream: Some(next),
        } = lane.downstream
        else {
            return None;
        };
        let next = next.raw();
        let next_lane = self.lanes.get(next)?;
        (matches!(next_lane.key, Some(TransportLaneKey::Belt { .. }))
            && lane.speed_subtiles_per_tick == next_lane.speed_subtiles_per_tick
            && self.upstream_by_slot[next].len() == 1)
            .then_some(next)
    }

    fn is_run_head(&self, slot: usize) -> bool {
        let upstreams = &self.upstream_by_slot[slot];
        !(upstreams.len() == 1 && self.chain_successor(upstreams[0].raw()) == Some(slot))
    }

    pub(in crate::simulation) fn run_count(&self) -> usize {
        self.run_records.len()
    }

    pub(in crate::simulation::belt_ops) fn run_for_slot(
        &self,
        index: TransportLaneIndex,
    ) -> Option<TransportRunIndex> {
        let lane = self.lanes.get(index.raw())?;
        lane.key?;
        let run = lane.run;
        (run != VACANT_SLOT).then(|| TransportRunIndex::from_index(run as usize))
    }

    pub(in crate::simulation::belt_ops) fn run_and_position_for_slot(
        &self,
        index: TransportLaneIndex,
    ) -> Option<(TransportRunIndex, usize)> {
        let run = self.run_for_slot(index)?;
        let position = self.lanes.get(index.raw())?.run_position;
        (position != VACANT_SLOT).then_some((run, position as usize))
    }

    /// Lanes of `run` in upstream-to-downstream order.
    pub(in crate::simulation::belt_ops) fn run_lanes(
        &self,
        run: TransportRunIndex,
    ) -> &[TransportLaneIndex] {
        let Some(record) = self.run_records.get(run.raw()) else {
            return &[];
        };
        let start = record.start as usize;
        &self.run_lane_slots[start..start + record.len as usize]
    }

    pub(in crate::simulation::belt_ops) fn run_is_cyclic(&self, run: TransportRunIndex) -> bool {
        self.run_records
            .get(run.raw())
            .is_some_and(|record| record.cyclic)
    }

    fn assign_slot(&mut self, key: TransportLaneKey, speed_subtiles_per_tick: u16) {
        let Some(raw) = lane_raw_index(key) else {
            return;
        };
        let slot = u32::try_from(self.lanes.len()).expect("transport lane slot capacity exceeded");
        self.lanes
            .push(TransportLaneRecord::occupied(key, speed_subtiles_per_tick));
        self.slot_by_raw[raw] = slot;
    }

    pub(in crate::simulation::belt_ops) fn slot_for(
        &self,
        key: TransportLaneKey,
    ) -> Option<TransportLaneIndex> {
        let raw = lane_raw_index(key)?;
        let &slot = self.slot_by_raw.get(raw)?;
        (slot != VACANT_SLOT).then(|| TransportLaneIndex::from_slot(slot as usize))
    }

    pub(in crate::simulation::belt_ops) fn upstream_for(
        &self,
        index: TransportLaneIndex,
    ) -> &[TransportLaneIndex] {
        self.upstream_by_slot
            .get(index.raw())
            .map(SmallVec::as_slice)
            .unwrap_or(&[])
    }

    pub(in crate::simulation::belt_ops) fn key_for(
        &self,
        index: TransportLaneIndex,
    ) -> Option<TransportLaneKey> {
        self.lane(index)?.key
    }

    pub(in crate::simulation::belt_ops) fn lane(
        &self,
        index: TransportLaneIndex,
    ) -> Option<TransportLaneRecord> {
        self.lanes
            .get(index.raw())
            .copied()
            .filter(|lane| lane.key.is_some())
    }

    fn push_upstream(&mut self, downstream: TransportLaneIndex, upstream: TransportLaneIndex) {
        if let Some(upstreams) = self.upstream_by_slot.get_mut(downstream.raw())
            && !upstreams.contains(&upstream)
        {
            upstreams.push(upstream);
        }
    }
}

fn downstream_targets(downstream: TransportLaneDownstream) -> SmallVec<[TransportLaneIndex; 2]> {
    match downstream {
        TransportLaneDownstream::Missing => SmallVec::new(),
        TransportLaneDownstream::Belt { downstream } => downstream.into_iter().collect(),
        TransportLaneDownstream::Splitter { outputs } => outputs.into_iter().flatten().collect(),
    }
}

fn transport_lane_index_len(entities: &EntityStore) -> usize {
    entities
        .transport_belts
        .keys()
        .chain(entities.splitters.keys())
        .filter_map(|entity_id| usize::try_from(entity_id.raw()).ok())
        .max()
        .and_then(|entity_index| entity_index.checked_add(1))
        .and_then(|entity_count| entity_count.checked_mul(4))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::TransportLaneRecord;

    #[test]
    fn dense_lane_hot_record_fits_one_cache_line() {
        assert!(
            std::mem::size_of::<TransportLaneRecord>() <= 64,
            "transport lane record grew to {} bytes",
            std::mem::size_of::<TransportLaneRecord>()
        );
    }
}
