use crate::simulation::rail_ops::{RailBlockPartition, RailGraph, RailSignalling};

/// Runtime rail state: the graph the placed rails form, the blocks the signals
/// cut it into, and what this tick's reservation pass made of both.
///
/// Everything here is derived from the placed rails and signals in
/// [`EntityStore`] — plus, for the signalling, from the trains — so it is
/// rebuilt rather than serialized, the same treatment
/// [`crate::simulation::circuit_state::CircuitSubsystem`] gets. The heat, fluid,
/// and robot subsystems keep a durable snapshot beside their topology because
/// their networks carry contents; a rail network holds nothing of its own.
///
/// A train's *claim* on a block is the one part of signalling that is not
/// derived, and it lives on the train rather than here. Which block a train was
/// heading for cannot be recovered from where it is standing, so the claim is
/// durable state; what is here is the index over those claims, which can be.
#[derive(Clone, Debug)]
pub(in crate::simulation) struct RailSubsystem {
    pub(in crate::simulation) graph_dirty: bool,
    pub(in crate::simulation) graph: RailGraph,
    /// The graph cut into blocks at the signal positions. Rebuilt with the
    /// graph, because a signal appearing changes both.
    pub(in crate::simulation) blocks: RailBlockPartition,
    pub(in crate::simulation) signalling: RailSignalling,
    #[cfg(test)]
    pub(in crate::simulation) graph_rebuilds: u64,
}

impl Default for RailSubsystem {
    fn default() -> Self {
        Self {
            graph_dirty: true,
            graph: RailGraph::default(),
            blocks: RailBlockPartition::default(),
            signalling: RailSignalling::default(),
            #[cfg(test)]
            graph_rebuilds: 0,
        }
    }
}

impl RailSubsystem {
    pub(in crate::simulation) fn invalidate(&mut self) {
        self.graph_dirty = true;
        self.graph = RailGraph::default();
        self.blocks = RailBlockPartition::default();
        self.signalling.clear();
    }

    pub(in crate::simulation) fn replace_graph(
        &mut self,
        graph: RailGraph,
        blocks: RailBlockPartition,
    ) {
        self.graph = graph;
        self.blocks = blocks;
        self.graph_dirty = false;
    }
}

impl_runtime_only_identity!(RailSubsystem);
