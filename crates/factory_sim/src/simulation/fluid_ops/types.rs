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
    pub(super) amount_milliunits: u64,
    pub(super) fluid_id: Option<FluidId>,
    pub(super) endpoints: Vec<FluidEndpoint>,
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

#[derive(Clone, Debug)]
pub(super) struct BuiltFluidNetwork {
    pub(super) network_id: u32,
    pub(super) boxes: Vec<FluidBoxKey>,
    pub(super) capacity_milliunits: u64,
    pub(super) total_milliunits: u64,
    pub(super) fluid_id: Option<FluidId>,
    pub(super) blocked: bool,
}
