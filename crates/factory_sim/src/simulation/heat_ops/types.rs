use crate::simulation::edge_geometry::EdgeEndpoint;
use crate::simulation::*;

/// One heat buffer as the topology builder sees it. Every heat entity owns
/// exactly one buffer, so the entity id is the buffer's identity.
#[derive(Clone, Debug)]
pub(super) struct HeatBufferNode {
    pub(super) entity_id: EntityId,
    pub(super) specific_heat_joules_per_degree: u64,
    pub(super) max_temperature_degrees: u32,
    pub(super) endpoints: Vec<EdgeEndpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::simulation) struct HeatNetworkBufferTopology {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) specific_heat_joules_per_degree: u64,
    /// Energy the buffer holds at its maximum temperature, above ambient.
    pub(in crate::simulation) capacity_joules: u64,
}

/// A connected set of heat buffers.
///
/// `buffers` is ordered by ascending maximum temperature (ties broken by entity
/// id for determinism) because the solve fills buffers in that order: once the
/// settling temperature fits the coolest remaining buffer it fits all the rest,
/// so one pass suffices. Sorting here keeps the per-tick solve allocation-free
/// and sort-free.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::simulation) struct HeatNetworkTopology {
    pub(in crate::simulation) network_id: u32,
    pub(in crate::simulation) buffers: Vec<HeatNetworkBufferTopology>,
    pub(in crate::simulation) specific_heat_joules_per_degree: u64,
    pub(in crate::simulation) capacity_joules: u64,
}
