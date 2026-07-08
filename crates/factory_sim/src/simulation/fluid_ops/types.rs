use crate::simulation::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct FluidBoxKey {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) box_index: usize,
}

#[derive(Clone, Debug)]
pub(super) struct FluidBoxNode {
    pub(super) key: FluidBoxKey,
    pub(super) capacity_milliunits: u64,
    pub(super) filter: Option<FluidId>,
    pub(super) endpoints: Vec<FluidEndpoint>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::simulation) struct FluidNetworkBoxTopology {
    pub(in crate::simulation) key: FluidBoxKey,
    pub(in crate::simulation) capacity_milliunits: u64,
    pub(in crate::simulation) filter: Option<FluidId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::simulation) struct FluidNetworkTopology {
    pub(in crate::simulation) network_id: u32,
    pub(in crate::simulation) boxes: Vec<FluidNetworkBoxTopology>,
    pub(in crate::simulation) capacity_milliunits: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::simulation) struct FluidNetworkDynamicSummary {
    pub(in crate::simulation) total_milliunits: u64,
    pub(in crate::simulation) fluid_id: Option<FluidId>,
    pub(in crate::simulation) blocked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct FluidEndpoint {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) axis: FluidEndpointAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum FluidEndpointAxis {
    Horizontal,
    Vertical,
}
