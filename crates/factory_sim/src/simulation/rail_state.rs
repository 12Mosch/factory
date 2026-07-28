use super::*;
use crate::simulation::rail_ops::RailGraph;
use std::collections::HashMap;

/// Cached rail graph.
///
/// Unlike the power, fluid, heat, and robot subsystems this holds *only* a
/// cache. Those four keep durable snapshots beside their topology because they
/// carry quantities that accumulate — stored energy, fluid amounts, robot
/// counts. Track carries nothing: a rail piece is its prototype and its
/// direction, so the whole graph is a pure function of the placed entities and
/// is rebuilt from them on load rather than saved.
///
/// It is therefore left out of the save, the state hash, and equality
/// altogether, the same treatment [`crate::simulation::circuit_state`] gives the
/// circuit topology it recomputes.
#[derive(Clone, Debug)]
pub(super) struct RailSubsystem {
    pub(super) graph: RailGraph,
    pub(super) graph_dirty: bool,
    pub(super) network_ids_by_entity: HashMap<EntityId, u32>,
    #[cfg(test)]
    pub(super) graph_rebuilds: u64,
}

// A fresh or freshly loaded subsystem starts dirty: it has no idea what track
// the world holds until it has looked.
impl Default for RailSubsystem {
    fn default() -> Self {
        Self {
            graph: RailGraph::default(),
            graph_dirty: true,
            network_ids_by_entity: HashMap::new(),
            #[cfg(test)]
            graph_rebuilds: 0,
        }
    }
}

impl RailSubsystem {
    pub(super) fn invalidate(&mut self) {
        self.graph_dirty = true;
    }

    pub(super) fn replace_graph(&mut self, graph: RailGraph) {
        self.network_ids_by_entity = network_ids_by_entity(&graph);
        self.graph = graph;
        self.graph_dirty = false;
    }
}

fn network_ids_by_entity(graph: &RailGraph) -> HashMap<EntityId, u32> {
    let mut network_ids_by_entity = HashMap::with_capacity(graph.edges.len());
    for network in &graph.networks {
        for edge in &network.edges {
            network_ids_by_entity.insert(graph.edges[*edge as usize].entity_id, network.network_id);
        }
    }
    network_ids_by_entity
}

impl_runtime_only_identity!(RailSubsystem);
