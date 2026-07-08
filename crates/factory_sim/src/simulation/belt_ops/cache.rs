use super::geometry::{belt_downstream_lane_key, splitter_output_lane_key};
use super::types::{TransportLaneDownstream, TransportLaneKey};
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneGraph {
    pub(in crate::simulation::belt_ops) lane_keys: Vec<TransportLaneKey>,
    downstream_by_index: Vec<TransportLaneDownstream>,
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

/// Subsystem-owned cache for belt/splitter transport.
///
/// This holds no authoritative simulation state: the durable belt/transport
/// data (lanes, item positions, splitter cursors) lives in [`EntityStore`].
/// The graph is a derived adjacency index rebuilt from `entities` whenever the
/// transport topology changes, and `visit_states` is per-tick DFS scratch.
/// All of it is `#[serde(skip)]` and reconstructed on load.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneCache {
    dirty: bool,
    pub(in crate::simulation) graph: TransportLaneGraph,
    pub(in crate::simulation) visit_states: TransportLaneVisitStorage,
    #[cfg(test)]
    pub(in crate::simulation) rebuilds: u64,
}

impl Default for TransportLaneCache {
    fn default() -> Self {
        Self {
            dirty: true,
            graph: TransportLaneGraph::default(),
            visit_states: TransportLaneVisitStorage::default(),
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
        self.dirty = false;
        #[cfg(test)]
        {
            self.rebuilds += 1;
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
                if let Some(slot) = self.downstream_by_index.get_mut(index) {
                    *slot = TransportLaneDownstream::Belt {
                        downstream: belt_downstream_lane_key(entities, entity_id, lane_index),
                    };
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
                    if let Some(slot) = self.downstream_by_index.get_mut(index) {
                        *slot = TransportLaneDownstream::Splitter {
                            outputs: [
                                splitter_output_lane_key(entities, entity_id, 0, lane_index),
                                splitter_output_lane_key(entities, entity_id, 1, lane_index),
                            ],
                        };
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
