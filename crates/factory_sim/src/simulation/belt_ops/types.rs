use super::*;

pub(in crate::simulation::belt_ops) const TRANSPORT_LANE_SLOTS_PER_ENTITY: usize = 4;
const SPLITTER_INPUT_PORTS: usize = 2;
const SPLITTER_LANES_PER_PORT: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) enum TransportLaneKey {
    Belt {
        entity_id: EntityId,
        lane_index: usize,
    },
    Splitter {
        entity_id: EntityId,
        input_port: usize,
        lane_index: usize,
    },
}

impl TransportLaneKey {
    pub(in crate::simulation) const fn entity_id(self) -> EntityId {
        match self {
            Self::Belt { entity_id, .. } | Self::Splitter { entity_id, .. } => entity_id,
        }
    }
}

/// Compact slot into the rebuilt lane graph's parallel arrays. Slot values
/// are only meaningful for the graph generation that assigned them; every
/// rebuild reassigns slots and re-derives all stored indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct TransportLaneIndex(u32);

impl TransportLaneIndex {
    pub(in crate::simulation::belt_ops) fn from_slot(slot: usize) -> Self {
        Self(u32::try_from(slot).expect("transport lane slot capacity exceeded"))
    }

    pub(in crate::simulation::belt_ops) const fn raw(self) -> usize {
        self.0 as usize
    }
}

/// Position of a lane in the sparse `entity_id * 4 + lane_offset` indirection
/// used to map wakeup keys onto compact slots. `None` when the key's
/// components could alias another entity's range.
pub(in crate::simulation::belt_ops) fn lane_raw_index(key: TransportLaneKey) -> Option<usize> {
    let (entity_id, lane_offset) = match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => (
            entity_id,
            (lane_index < TRANSPORT_LANE_SLOTS_PER_ENTITY).then_some(lane_index)?,
        ),
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => {
            if input_port >= SPLITTER_INPUT_PORTS || lane_index >= SPLITTER_LANES_PER_PORT {
                return None;
            }
            let lane_offset = input_port
                .checked_mul(SPLITTER_LANES_PER_PORT)?
                .checked_add(lane_index)?;
            (
                entity_id,
                (lane_offset < TRANSPORT_LANE_SLOTS_PER_ENTITY).then_some(lane_offset)?,
            )
        }
    };
    let entity_index = usize::try_from(entity_id.raw()).ok()?;
    entity_index
        .checked_mul(TRANSPORT_LANE_SLOTS_PER_ENTITY)?
        .checked_add(lane_offset)
}

/// Compact id of a lane run: a maximal chain of belt lanes that is advanced
/// as one unit, stored in upstream-to-downstream lane order. Splitter lanes, lanes
/// at sideload merge points, and lanes fed by splitters start their own runs.
/// Like [`TransportLaneIndex`], run ids are only meaningful for the graph
/// generation that assigned them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct TransportRunIndex(u32);

impl TransportRunIndex {
    pub(in crate::simulation::belt_ops) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect("transport run capacity exceeded"))
    }

    pub(in crate::simulation::belt_ops) const fn raw(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) enum TransportRunTraversalStep {
    Enter(TransportRunIndex),
    Exit {
        run: TransportRunIndex,
        /// Resolved while entering the run so post-order exit does not repeat
        /// the dense-lane key lookup for the run tail.
        tail_key: TransportLaneKey,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation::belt_ops) enum TransportEndpoint {
    Belt {
        entity_id: EntityId,
    },
    Splitter {
        entity_id: EntityId,
        input_port: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation::belt_ops) enum TransportRunVisitState {
    Processing,
    Done,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) enum TransportLaneDownstream {
    #[default]
    Missing,
    Belt {
        downstream: Option<TransportLaneIndex>,
    },
    Splitter {
        outputs: [Option<TransportLaneIndex>; 2],
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_raw_index_rejects_components_that_alias_the_next_entity() {
        let entity_id = EntityId::new(7);

        assert!(
            lane_raw_index(TransportLaneKey::Belt {
                entity_id,
                lane_index: 3,
            })
            .is_some()
        );
        assert!(
            lane_raw_index(TransportLaneKey::Belt {
                entity_id,
                lane_index: 4,
            })
            .is_none()
        );
        assert!(
            lane_raw_index(TransportLaneKey::Splitter {
                entity_id,
                input_port: 1,
                lane_index: 1,
            })
            .is_some()
        );
        assert!(
            lane_raw_index(TransportLaneKey::Splitter {
                entity_id,
                input_port: 2,
                lane_index: 0,
            })
            .is_none()
        );
        assert!(
            lane_raw_index(TransportLaneKey::Splitter {
                entity_id,
                input_port: 0,
                lane_index: 2,
            })
            .is_none()
        );
    }
}
