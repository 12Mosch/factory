use super::geometry::{belt_downstream_lane_key, splitter_output_lane_key};
use super::types::{
    TRANSPORT_LANE_SLOTS_PER_ENTITY, TransportLaneDownstream, TransportLaneIndex, TransportLaneKey,
    TransportRunIndex, TransportRunTraversalStep, lane_raw_index,
};
use super::*;
use crate::logistics::BeltItemId;

const VACANT_SLOT: u32 = u32::MAX;

/// Most scoped edits carried between refreshes before the cache falls back to
/// a full rebuild.
const MAX_DIRTY_REGIONS: usize = 32;
const PATCH_STORAGE_HEADROOM: usize = MAX_DIRTY_REGIONS * TRANSPORT_LANE_SLOTS_PER_ENTITY;

/// One transport-affecting entity edit since the last refresh. The patch
/// re-resolves lane geometry for entities whose downstream resolution can see
/// these tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportDirtyRegion {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) footprint: EntityFootprint,
}

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) struct TransportRunVisitSlot {
    pub(in crate::simulation::belt_ops) generation: u32,
    pub(in crate::simulation::belt_ops) state: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportRunVisitStorage {
    pub(in crate::simulation::belt_ops) generation: u32,
    pub(in crate::simulation::belt_ops) states: Vec<TransportRunVisitSlot>,
    pub(in crate::simulation::belt_ops) traversal_stack: Vec<TransportRunTraversalStep>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) struct TransportRunActiveSlot {
    active_generation: u32,
    pending_generation: u32,
    active_start_position: u32,
    pending_start_position: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportRunActiveStorage {
    active_generation: u32,
    pending_generation: u32,
    /// Current belt-phase work queue. After `finish_tick`, this becomes the
    /// next tick's queue and may receive producer/pickup wakeups via
    /// `mark_active` until the next belt phase begins.
    pub(in crate::simulation::belt_ops) runs: Vec<TransportRunIndex>,
    pending_runs: Vec<TransportRunIndex>,
    marks: Vec<TransportRunActiveSlot>,
}

#[derive(Clone, Copy)]
enum TransportRunQueue {
    Active,
    Pending,
}

/// Subsystem-owned cache for belt/splitter transport.
///
/// This holds no authoritative simulation state: the durable belt/transport
/// data (lanes, item positions, splitter cursors) lives in [`EntityStore`].
/// The graph is a derived adjacency index rebuilt from `entities` whenever the
/// transport topology changes, `active_runs` is the advancement work queue,
/// and `visit_states` is reusable per-tick traversal scratch.
/// All of it is `#[serde(skip)]` and reconstructed on load.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneCache {
    dirty: bool,
    /// Scoped edits since the last refresh, applied as an incremental patch
    /// unless `dirty` forces a full rebuild.
    dirty_regions: Vec<TransportDirtyRegion>,
    /// Monotonic change tokens consumed by incremental presentation. These
    /// are derived runtime state; saves reconstruct presentation from scratch.
    pub(in crate::simulation) item_revision: u64,
    pub(in crate::simulation) item_revisions_by_entity: Vec<u64>,
    next_item_id: u64,
    pub(in crate::simulation) graph: TransportLaneGraph,
    pub(in crate::simulation) visit_states: TransportRunVisitStorage,
    pub(in crate::simulation) active_runs: TransportRunActiveStorage,
    #[cfg(test)]
    pub(in crate::simulation) rebuilds: u64,
    #[cfg(test)]
    pub(in crate::simulation) patches: u64,
}

impl Default for TransportLaneCache {
    fn default() -> Self {
        Self {
            dirty: true,
            dirty_regions: Vec::new(),
            item_revision: 0,
            item_revisions_by_entity: Vec::new(),
            next_item_id: 1,
            graph: TransportLaneGraph::default(),
            visit_states: TransportRunVisitStorage::default(),
            active_runs: TransportRunActiveStorage::default(),
            #[cfg(test)]
            rebuilds: 0,
            #[cfg(test)]
            patches: 0,
        }
    }
}

impl TransportLaneCache {
    pub(in crate::simulation) fn initialize_item_tracking(&mut self, entities: &EntityStore) {
        self.next_item_id = entities
            .transport_belts
            .values()
            .flat_map(|segment| segment.lanes.iter())
            .flat_map(|lane| lane.items.iter())
            .chain(
                entities
                    .splitters
                    .values()
                    .flat_map(|state| state.input_lanes.iter())
                    .flat_map(|lanes| lanes.iter())
                    .flat_map(|lane| lane.items.iter()),
            )
            .map(|item| item.id.raw())
            .max()
            .map_or(1, |max_id| max_id.checked_add(1).unwrap_or(0));
    }

    pub(in crate::simulation) fn allocate_item_id(&mut self) -> BeltItemId {
        assert_ne!(self.next_item_id, 0, "belt item identity space exhausted");
        let id = BeltItemId::new(self.next_item_id);
        self.next_item_id = self
            .next_item_id
            .checked_add(1)
            .expect("belt item identity space exhausted");
        id
    }

    pub(in crate::simulation) fn mark_items_changed(&mut self, entity_id: EntityId) {
        mark_item_revision(
            &mut self.item_revision,
            &mut self.item_revisions_by_entity,
            entity_id,
        );
    }

    pub(in crate::simulation) fn item_revision(&self) -> u64 {
        self.item_revision
    }

    pub(in crate::simulation) fn entity_item_revision(&self, entity_id: EntityId) -> u64 {
        usize::try_from(entity_id.raw())
            .ok()
            .and_then(|index| self.item_revisions_by_entity.get(index))
            .copied()
            .unwrap_or(0)
    }

    pub(in crate::simulation) fn invalidate(&mut self) {
        self.dirty = true;
        self.dirty_regions.clear();
    }

    pub(in crate::simulation) fn invalidate_region(&mut self, region: TransportDirtyRegion) {
        if self.dirty {
            return;
        }
        if self.dirty_regions.len() >= MAX_DIRTY_REGIONS {
            self.invalidate();
            return;
        }
        self.dirty_regions.push(region);
    }

    pub(in crate::simulation) fn refresh(
        &mut self,
        entities: &EntityStore,
        catalog_underground_distance: u8,
    ) {
        if self.dirty {
            self.rebuild_all(entities);
            return;
        }
        if self.dirty_regions.is_empty() {
            return;
        }

        let regions = std::mem::take(&mut self.dirty_regions);
        match self
            .graph
            .patch(entities, &regions, catalog_underground_distance)
        {
            Some(new_runs) => {
                for run in new_runs {
                    self.activate_run_from_items(entities, TransportRunIndex::from_index(run));
                }
                #[cfg(test)]
                {
                    self.patches += 1;
                }
            }
            None => self.rebuild_all(entities),
        }
    }

    fn rebuild_all(&mut self, entities: &EntityStore) {
        self.graph.rebuild(entities);
        self.active_runs
            .rebuild_from_entities(entities, &self.graph);
        self.dirty = false;
        self.dirty_regions.clear();
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
    }

    /// Wakes a run created by an incremental patch at its most upstream lane
    /// that holds items, mirroring what a full active-set rebuild derives.
    fn activate_run_from_items(&mut self, entities: &EntityStore, run: TransportRunIndex) {
        let position =
            self.graph
                .run_lanes(run)
                .iter()
                .enumerate()
                .find_map(|(position, &slot)| {
                    let key = self.graph.key_for(slot)?;
                    let has_items = match key {
                        TransportLaneKey::Belt {
                            entity_id,
                            lane_index,
                        } => entities
                            .transport_belts
                            .get(&entity_id)
                            .and_then(|segment| segment.lanes.get(lane_index))
                            .is_some_and(|lane| !lane.items.is_empty()),
                        TransportLaneKey::Splitter {
                            entity_id,
                            input_port,
                            lane_index,
                        } => entities
                            .splitters
                            .get(&entity_id)
                            .and_then(|state| state.input_lanes.get(input_port))
                            .and_then(|lanes| lanes.get(lane_index))
                            .is_some_and(|lane| !lane.items.is_empty()),
                    };
                    has_items.then_some(position)
                });
        if let Some(position) = position {
            self.active_runs.mark_active(run, position);
        }
    }

    pub(in crate::simulation) fn mark_active(&mut self, key: TransportLaneKey) {
        if let Some(index) = self.graph.slot_for(key)
            && let Some((run, position)) = self.graph.run_and_position_for_slot(index)
        {
            self.active_runs.mark_active(run, position);
        }
    }

    pub(in crate::simulation) fn mark_active_with_upstreams(&mut self, key: TransportLaneKey) {
        let Some(index) = self.graph.slot_for(key) else {
            return;
        };
        if let Some((run, position)) = self.graph.run_and_position_for_slot(index) {
            self.active_runs.mark_active(run, position);
        }
        for &upstream in self.graph.upstream_for(index) {
            if let Some((run, position)) = self.graph.run_and_position_for_slot(upstream) {
                self.active_runs.mark_active(run, position);
            }
        }
    }
}

pub(in crate::simulation::belt_ops) fn mark_item_revision(
    item_revision: &mut u64,
    item_revisions_by_entity: &mut Vec<u64>,
    entity_id: EntityId,
) {
    *item_revision = item_revision.wrapping_add(1);
    if *item_revision == 0 {
        *item_revision = 1;
    }
    let Ok(index) = usize::try_from(entity_id.raw()) else {
        return;
    };
    if item_revisions_by_entity.len() <= index {
        item_revisions_by_entity.resize(index + 1, 0);
    }
    item_revisions_by_entity[index] = *item_revision;
}

impl TransportLaneGraph {
    fn rebuild(&mut self, entities: &EntityStore) {
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
    fn patch(
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

impl TransportRunVisitStorage {
    pub(in crate::simulation) fn begin_tick(&mut self, required_len: usize) {
        if self.states.len() < required_len {
            self.states.reserve(
                required_len
                    .saturating_sub(self.states.len())
                    .saturating_add(PATCH_STORAGE_HEADROOM),
            );
            self.states
                .resize(required_len, TransportRunVisitSlot::default());
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.states.fill(TransportRunVisitSlot::default());
            self.generation = 1;
        }
    }
}

impl TransportRunActiveStorage {
    fn rebuild_from_entities(&mut self, entities: &EntityStore, graph: &TransportLaneGraph) {
        advance_active_generation(&mut self.active_generation, &mut self.marks);
        self.runs.clear();

        let required_len = graph.run_count();
        if self.marks.len() < required_len {
            self.marks.reserve(
                required_len
                    .saturating_sub(self.marks.len())
                    .saturating_add(PATCH_STORAGE_HEADROOM),
            );
            self.marks
                .resize(required_len, TransportRunActiveSlot::default());
        }

        for (&entity_id, segment) in &entities.transport_belts {
            for (lane_index, lane) in segment.lanes.iter().enumerate() {
                if !lane.items.is_empty() {
                    let key = TransportLaneKey::Belt {
                        entity_id,
                        lane_index,
                    };
                    if let Some(index) = graph.slot_for(key)
                        && let Some((run, position)) = graph.run_and_position_for_slot(index)
                    {
                        self.mark_active(run, position);
                    }
                }
            }
        }

        for (&entity_id, state) in &entities.splitters {
            for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
                for (lane_index, lane) in input_lanes.iter().enumerate() {
                    if !lane.items.is_empty() {
                        let key = TransportLaneKey::Splitter {
                            entity_id,
                            input_port,
                            lane_index,
                        };
                        if let Some(index) = graph.slot_for(key)
                            && let Some((run, position)) = graph.run_and_position_for_slot(index)
                        {
                            self.mark_active(run, position);
                        }
                    }
                }
            }
        }
    }

    pub(in crate::simulation) fn begin_tick(&mut self, required_len: usize) {
        if self.marks.len() < required_len {
            self.marks.reserve(
                required_len
                    .saturating_sub(self.marks.len())
                    .saturating_add(PATCH_STORAGE_HEADROOM),
            );
            self.marks
                .resize(required_len, TransportRunActiveSlot::default());
        }
        advance_pending_generation(&mut self.pending_generation, &mut self.marks);
        self.pending_runs.clear();
    }

    pub(in crate::simulation) fn finish_tick(&mut self) {
        advance_active_generation(&mut self.active_generation, &mut self.marks);

        self.runs.clear();
        self.runs.reserve(self.pending_runs.len());
        let mut pending_runs = std::mem::take(&mut self.pending_runs);
        for run in pending_runs.drain(..) {
            let start_position = self.marks[run.raw()].pending_start_position as usize;
            self.mark_active(run, start_position);
        }
        self.pending_runs = pending_runs;
    }

    pub(in crate::simulation::belt_ops) fn active_start_position(
        &self,
        run: TransportRunIndex,
    ) -> usize {
        self.marks
            .get(run.raw())
            .filter(|mark| mark.active_generation == self.active_generation)
            .map_or(0, |mark| mark.active_start_position as usize)
    }

    pub(in crate::simulation::belt_ops) fn mark_pending(
        &mut self,
        run: TransportRunIndex,
        start_position: usize,
    ) {
        mark_active_run(
            &mut self.marks,
            self.pending_generation,
            run,
            start_position,
            &mut self.pending_runs,
            TransportRunQueue::Pending,
        );
    }

    fn mark_active(&mut self, run: TransportRunIndex, start_position: usize) {
        mark_active_run(
            &mut self.marks,
            self.active_generation,
            run,
            start_position,
            &mut self.runs,
            TransportRunQueue::Active,
        );
    }
}

fn advance_active_generation(generation: &mut u32, marks: &mut [TransportRunActiveSlot]) {
    advance_generation(generation, marks, |mark| {
        mark.active_generation = 0;
    });
}

fn advance_pending_generation(generation: &mut u32, marks: &mut [TransportRunActiveSlot]) {
    advance_generation(generation, marks, |mark| {
        mark.pending_generation = 0;
    });
}

fn advance_generation(
    generation: &mut u32,
    marks: &mut [TransportRunActiveSlot],
    reset_mark: impl Fn(&mut TransportRunActiveSlot),
) {
    *generation = generation.wrapping_add(1);
    if *generation == 0 {
        for mark in marks {
            reset_mark(mark);
        }
        *generation = 1;
    }
}

fn mark_active_run(
    marks: &mut Vec<TransportRunActiveSlot>,
    generation: u32,
    index: TransportRunIndex,
    start_position: usize,
    runs: &mut Vec<TransportRunIndex>,
    queue: TransportRunQueue,
) {
    if marks.len() <= index.raw() {
        marks.resize(index.raw() + 1, TransportRunActiveSlot::default());
    }
    let Some(mark) = marks.get_mut(index.raw()) else {
        return;
    };
    let start_position =
        u32::try_from(start_position).expect("transport run position capacity exceeded");
    let (current_generation, current_start_position) = match queue {
        TransportRunQueue::Active => (mark.active_generation, mark.active_start_position),
        TransportRunQueue::Pending => (mark.pending_generation, mark.pending_start_position),
    };
    if current_generation == generation {
        if start_position < current_start_position {
            match queue {
                TransportRunQueue::Active => mark.active_start_position = start_position,
                TransportRunQueue::Pending => mark.pending_start_position = start_position,
            }
        }
        return;
    }
    match queue {
        TransportRunQueue::Active => {
            mark.active_generation = generation;
            mark.active_start_position = start_position;
        }
        TransportRunQueue::Pending => {
            mark.pending_generation = generation;
            mark.pending_start_position = start_position;
        }
    }
    runs.push(index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_lane_hot_record_fits_one_cache_line() {
        assert!(
            std::mem::size_of::<TransportLaneRecord>() <= 64,
            "transport lane record grew to {} bytes",
            std::mem::size_of::<TransportLaneRecord>()
        );
    }
}
