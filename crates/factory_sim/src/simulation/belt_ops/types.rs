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
        downstream: Option<TransportLaneKey>,
    },
    Splitter {
        outputs: [Option<TransportLaneKey>; 2],
    },
}
