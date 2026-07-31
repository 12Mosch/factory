use std::collections::BTreeMap;

use crate::ids::EntityId;
use crate::rolling_stock::RailTarget;
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
    /// The mark each placed train stop puts on the track, in stop-entity order.
    ///
    /// Derived from where the stop stands and what track is beside it, and
    /// therefore rebuilt with the graph: a stop whose rail was mined out from
    /// under it simply drops out of here, and one that had none until the player
    /// laid some appears. Deriving it is what stops a stop from ever naming a
    /// rail that is no longer there — the state a durable mark would have to be
    /// pruned out of.
    pub(in crate::simulation) stop_targets: BTreeMap<EntityId, RailTarget>,
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
            stop_targets: BTreeMap::new(),
            #[cfg(test)]
            graph_rebuilds: 0,
        }
    }
}

impl RailSubsystem {
    /// Drops what the rebuild will replace, and marks it as owed.
    ///
    /// The stop marks are deliberately *not* dropped here. They are what the
    /// next rebuild compares against to find the platforms that have moved, and
    /// a train booked into one of those has to be told; clearing them would make
    /// every stop look newly bound and nothing look moved. Nothing reads them
    /// while the graph is dirty — [`crate::simulation::Simulation::
    /// train_stop_target`] says so — so holding a description of the world as it
    /// was until the rebuild replaces it costs nothing.
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
        stop_targets: BTreeMap<EntityId, RailTarget>,
    ) {
        self.graph = graph;
        self.blocks = blocks;
        self.stop_targets = stop_targets;
        self.graph_dirty = false;
    }
}

impl_runtime_only_identity!(RailSubsystem);
