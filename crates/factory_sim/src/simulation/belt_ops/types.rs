use super::*;

const TRANSPORT_LANE_SLOTS_PER_ENTITY: usize = 4;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct TransportLaneIndex(usize);

impl TransportLaneIndex {
    pub(in crate::simulation::belt_ops) fn from_key(key: TransportLaneKey) -> Option<Self> {
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
            .map(Self)
    }

    pub(in crate::simulation::belt_ops) const fn raw(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation::belt_ops) enum TransportLaneTraversalStep {
    Enter(TransportLaneIndex),
    Exit(TransportLaneIndex),
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
pub(in crate::simulation::belt_ops) enum BeltLaneVisitState {
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
    fn lane_index_rejects_components_that_alias_the_next_entity() {
        let entity_id = EntityId::new(7);

        assert!(
            TransportLaneIndex::from_key(TransportLaneKey::Belt {
                entity_id,
                lane_index: 3,
            })
            .is_some()
        );
        assert!(
            TransportLaneIndex::from_key(TransportLaneKey::Belt {
                entity_id,
                lane_index: 4,
            })
            .is_none()
        );
        assert!(
            TransportLaneIndex::from_key(TransportLaneKey::Splitter {
                entity_id,
                input_port: 1,
                lane_index: 1,
            })
            .is_some()
        );
        assert!(
            TransportLaneIndex::from_key(TransportLaneKey::Splitter {
                entity_id,
                input_port: 2,
                lane_index: 0,
            })
            .is_none()
        );
        assert!(
            TransportLaneIndex::from_key(TransportLaneKey::Splitter {
                entity_id,
                input_port: 0,
                lane_index: 2,
            })
            .is_none()
        );
    }
}
