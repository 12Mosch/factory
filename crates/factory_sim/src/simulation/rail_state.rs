use crate::simulation::rail_ops::RailGraph;

/// Runtime rail state: the graph the placed rails form.
///
/// Everything here is derived from the placed rails in [`EntityStore`], so it is
/// rebuilt on load rather than serialized — the same treatment
/// [`crate::simulation::circuit_state::CircuitSubsystem`] gets. The heat, fluid,
/// and robot subsystems keep a durable snapshot beside their topology because
/// their networks carry contents; a rail network holds nothing of its own, so
/// there is nothing to save.
#[derive(Clone, Debug)]
pub(in crate::simulation) struct RailSubsystem {
    pub(in crate::simulation) graph_dirty: bool,
    pub(in crate::simulation) graph: RailGraph,
    #[cfg(test)]
    pub(in crate::simulation) graph_rebuilds: u64,
}

impl Default for RailSubsystem {
    fn default() -> Self {
        Self {
            graph_dirty: true,
            graph: RailGraph::default(),
            #[cfg(test)]
            graph_rebuilds: 0,
        }
    }
}

impl RailSubsystem {
    pub(in crate::simulation) fn invalidate(&mut self) {
        self.graph_dirty = true;
        self.graph = RailGraph::default();
    }

    pub(in crate::simulation) fn replace_graph(&mut self, graph: RailGraph) {
        self.graph = graph;
        self.graph_dirty = false;
    }
}

impl_runtime_only_identity!(RailSubsystem);
