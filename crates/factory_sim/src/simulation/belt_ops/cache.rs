use super::geometry::{belt_downstream_lane_key, splitter_output_lane_key};
use super::types::{TransportLaneDownstream, TransportLaneKey};
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneGraph {
    pub(in crate::simulation::belt_ops) lane_keys: Vec<TransportLaneKey>,
    downstream_by_index: Vec<TransportLaneDownstream>,
    upstream_by_index: Vec<SmallVec<[TransportLaneKey; 2]>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) struct TransportLaneVisitSlot {
    pub(in crate::simulation::belt_ops) generation: u32,
    pub(in crate::simulation::belt_ops) state: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneVisitStorage {
    pub(in crate::simulation::belt_ops) generation: u32,
    pub(in crate::simulation::belt_ops) states: Vec<TransportLaneVisitSlot>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) struct TransportLaneActiveSlot {
    active_generation: u32,
    pending_generation: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneActiveStorage {
    active_generation: u32,
    pending_generation: u32,
    /// Current belt-phase work queue. After `finish_tick`, this becomes the
    /// next tick's queue and may receive producer/pickup wakeups via
    /// `mark_active` until the next belt phase begins.
    pub(in crate::simulation::belt_ops) lanes: Vec<TransportLaneKey>,
    pending_lanes: Vec<TransportLaneKey>,
    marks: Vec<TransportLaneActiveSlot>,
}

/// Subsystem-owned cache for belt/splitter transport.
///
/// This holds no authoritative simulation state: the durable belt/transport
/// data (lanes, item positions, splitter cursors) lives in [`EntityStore`].
/// The graph is a derived adjacency index rebuilt from `entities` whenever the
/// transport topology changes, `active_lanes` is the advancement work queue,
/// and `visit_states` is per-tick DFS scratch.
/// All of it is `#[serde(skip)]` and reconstructed on load.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneCache {
    dirty: bool,
    pub(in crate::simulation) graph: TransportLaneGraph,
    pub(in crate::simulation) visit_states: TransportLaneVisitStorage,
    pub(in crate::simulation) active_lanes: TransportLaneActiveStorage,
    #[cfg(test)]
    pub(in crate::simulation) rebuilds: u64,
}

impl Default for TransportLaneCache {
    fn default() -> Self {
        Self {
            dirty: true,
            graph: TransportLaneGraph::default(),
            visit_states: TransportLaneVisitStorage::default(),
            active_lanes: TransportLaneActiveStorage::default(),
            #[cfg(test)]
            rebuilds: 0,
        }
    }
}

impl TransportLaneCache {
    pub(in crate::simulation) fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub(in crate::simulation) fn refresh(&mut self, entities: &EntityStore) {
        if !self.dirty {
            return;
        }

        self.graph.rebuild(entities);
        self.active_lanes.rebuild_from_entities(entities);
        self.dirty = false;
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
    }

    pub(in crate::simulation) fn mark_active(&mut self, key: TransportLaneKey) {
        self.active_lanes.mark_active(key);
    }

    pub(in crate::simulation) fn mark_active_with_upstreams(&mut self, key: TransportLaneKey) {
        self.active_lanes.mark_active(key);
        for upstream in self.graph.upstream_for(key) {
            self.active_lanes.mark_active(*upstream);
        }
    }
}

impl TransportLaneGraph {
    fn rebuild(&mut self, entities: &EntityStore) {
        let lane_count = transport_lane_index_len(entities);
        self.lane_keys.clear();
        self.lane_keys.reserve(
            entities
                .transport_belts
                .len()
                .saturating_mul(2)
                .saturating_add(entities.splitters.len().saturating_mul(4)),
        );
        self.downstream_by_index.clear();
        self.downstream_by_index
            .resize(lane_count, TransportLaneDownstream::Missing);
        self.upstream_by_index.clear();
        self.upstream_by_index
            .resize_with(lane_count, SmallVec::new);

        for &entity_id in entities.transport_belts.keys() {
            for lane_index in 0..2 {
                let key = TransportLaneKey::Belt {
                    entity_id,
                    lane_index,
                };
                self.lane_keys.push(key);
                let Some(index) = visit_state_index(key) else {
                    continue;
                };
                let downstream = belt_downstream_lane_key(entities, entity_id, lane_index);
                if let Some(slot) = self.downstream_by_index.get_mut(index) {
                    *slot = TransportLaneDownstream::Belt { downstream };
                }
                if let Some(downstream) = downstream {
                    self.push_upstream(downstream, key);
                }
            }
        }

        for &entity_id in entities.splitters.keys() {
            for input_port in 0..2 {
                for lane_index in 0..2 {
                    let key = TransportLaneKey::Splitter {
                        entity_id,
                        input_port,
                        lane_index,
                    };
                    self.lane_keys.push(key);
                    let Some(index) = visit_state_index(key) else {
                        continue;
                    };
                    let outputs = [
                        splitter_output_lane_key(entities, entity_id, 0, lane_index),
                        splitter_output_lane_key(entities, entity_id, 1, lane_index),
                    ];
                    if let Some(slot) = self.downstream_by_index.get_mut(index) {
                        *slot = TransportLaneDownstream::Splitter { outputs };
                    }
                    for output in outputs.into_iter().flatten() {
                        self.push_upstream(output, key);
                    }
                }
            }
        }
    }

    pub(in crate::simulation::belt_ops) fn downstream_for(
        &self,
        key: TransportLaneKey,
    ) -> TransportLaneDownstream {
        visit_state_index(key)
            .and_then(|index| self.downstream_by_index.get(index))
            .copied()
            .unwrap_or(TransportLaneDownstream::Missing)
    }

    pub(in crate::simulation::belt_ops) fn upstream_for(
        &self,
        key: TransportLaneKey,
    ) -> &[TransportLaneKey] {
        visit_state_index(key)
            .and_then(|index| self.upstream_by_index.get(index))
            .map(SmallVec::as_slice)
            .unwrap_or(&[])
    }

    fn push_upstream(&mut self, downstream: TransportLaneKey, upstream: TransportLaneKey) {
        let Some(index) = visit_state_index(downstream) else {
            return;
        };
        if let Some(upstreams) = self.upstream_by_index.get_mut(index)
            && !upstreams.contains(&upstream)
        {
            upstreams.push(upstream);
        }
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

impl TransportLaneVisitStorage {
    pub(in crate::simulation) fn begin_tick(&mut self, required_len: usize) {
        if self.states.len() < required_len {
            self.states
                .resize(required_len, TransportLaneVisitSlot::default());
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.states.fill(TransportLaneVisitSlot::default());
            self.generation = 1;
        }
    }
}

impl TransportLaneActiveStorage {
    fn rebuild_from_entities(&mut self, entities: &EntityStore) {
        advance_active_generation(&mut self.active_generation, &mut self.marks);
        self.lanes.clear();

        let required_len = transport_lane_index_len(entities);
        if self.marks.len() < required_len {
            self.marks
                .resize(required_len, TransportLaneActiveSlot::default());
        }

        for (&entity_id, segment) in &entities.transport_belts {
            for (lane_index, lane) in segment.lanes.iter().enumerate() {
                if !lane.items.is_empty() {
                    self.mark_active(TransportLaneKey::Belt {
                        entity_id,
                        lane_index,
                    });
                }
            }
        }

        for (&entity_id, state) in &entities.splitters {
            for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
                for (lane_index, lane) in input_lanes.iter().enumerate() {
                    if !lane.items.is_empty() {
                        self.mark_active(TransportLaneKey::Splitter {
                            entity_id,
                            input_port,
                            lane_index,
                        });
                    }
                }
            }
        }
    }

    pub(in crate::simulation) fn begin_tick(&mut self, required_len: usize) {
        if self.marks.len() < required_len {
            self.marks
                .resize(required_len, TransportLaneActiveSlot::default());
        }
        advance_pending_generation(&mut self.pending_generation, &mut self.marks);
        self.pending_lanes.clear();
    }

    pub(in crate::simulation) fn finish_tick(&mut self) {
        advance_active_generation(&mut self.active_generation, &mut self.marks);

        self.lanes.clear();
        self.lanes.reserve(self.pending_lanes.len());
        let mut pending_lanes = std::mem::take(&mut self.pending_lanes);
        for key in pending_lanes.drain(..) {
            self.mark_active(key);
        }
        self.pending_lanes = pending_lanes;
    }

    pub(in crate::simulation::belt_ops) fn mark_pending(&mut self, key: TransportLaneKey) {
        mark_active_lane(
            &mut self.marks,
            self.pending_generation,
            key,
            &mut self.pending_lanes,
            |mark| mark.pending_generation,
            |mark, generation| mark.pending_generation = generation,
        );
    }

    fn mark_active(&mut self, key: TransportLaneKey) {
        mark_active_lane(
            &mut self.marks,
            self.active_generation,
            key,
            &mut self.lanes,
            |mark| mark.active_generation,
            |mark, generation| mark.active_generation = generation,
        );
    }
}

fn advance_active_generation(generation: &mut u32, marks: &mut [TransportLaneActiveSlot]) {
    advance_generation(generation, marks, |mark| {
        mark.active_generation = 0;
    });
}

fn advance_pending_generation(generation: &mut u32, marks: &mut [TransportLaneActiveSlot]) {
    advance_generation(generation, marks, |mark| {
        mark.pending_generation = 0;
    });
}

fn advance_generation(
    generation: &mut u32,
    marks: &mut [TransportLaneActiveSlot],
    reset_mark: impl Fn(&mut TransportLaneActiveSlot),
) {
    *generation = generation.wrapping_add(1);
    if *generation == 0 {
        for mark in marks {
            reset_mark(mark);
        }
        *generation = 1;
    }
}

fn mark_active_lane(
    marks: &mut Vec<TransportLaneActiveSlot>,
    generation: u32,
    key: TransportLaneKey,
    lanes: &mut Vec<TransportLaneKey>,
    current_generation: impl Fn(&TransportLaneActiveSlot) -> u32,
    set_generation: impl Fn(&mut TransportLaneActiveSlot, u32),
) {
    let Some(index) = visit_state_index(key) else {
        return;
    };
    if marks.len() <= index {
        marks.resize(index + 1, TransportLaneActiveSlot::default());
    }
    let Some(mark) = marks.get_mut(index) else {
        return;
    };
    if current_generation(mark) == generation {
        return;
    }
    set_generation(mark, generation);
    lanes.push(key);
}

pub(in crate::simulation::belt_ops) fn visit_state_index(key: TransportLaneKey) -> Option<usize> {
    let (entity_id, lane_offset) = match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => (entity_id, lane_index),
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => (
            entity_id,
            input_port.checked_mul(2)?.checked_add(lane_index)?,
        ),
    };
    let entity_index = usize::try_from(entity_id.raw()).ok()?;
    entity_index.checked_mul(4)?.checked_add(lane_offset)
}
