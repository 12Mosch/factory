use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EntityTopologyImpact {
    pub(crate) affects_power_topology: bool,
    pub(crate) affects_transport_lane_graph: bool,
}

pub(crate) fn impact_for_prototype(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
) -> EntityTopologyImpact {
    let Some(prototype) = sim.world.prototypes.entity(prototype_id) else {
        return EntityTopologyImpact::default();
    };

    EntityTopologyImpact {
        affects_power_topology: sim.prototype_affects_power_topology(prototype),
        affects_transport_lane_graph: sim.prototype_affects_transport_lane_graph(prototype),
    }
}

pub(crate) fn apply_entity_topology_change(
    sim: &mut Simulation,
    impact: EntityTopologyImpact,
    entity_id: EntityId,
    footprint: EntityFootprint,
) {
    if impact.affects_power_topology {
        sim.invalidate_power_state();
    }
    if impact.affects_transport_lane_graph {
        sim.invalidate_transport_lane_graph_region(entity_id, footprint);
    }
    sim.invalidate_fluid_state();
    sim.bump_entity_topology_revision();
}
