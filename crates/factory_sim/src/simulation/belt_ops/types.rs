use super::*;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct TransportLaneIndex(usize);

impl TransportLaneIndex {
    pub(in crate::simulation::belt_ops) fn from_key(key: TransportLaneKey) -> Option<Self> {
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
        entity_index
            .checked_mul(4)?
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
