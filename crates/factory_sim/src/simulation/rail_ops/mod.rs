pub mod blocks;
pub mod geometry;
mod graph_builder;
mod network_access;
mod pathfinding;
mod signalling;
#[cfg(test)]
mod test_graphs;
mod types;

pub(in crate::simulation) use blocks::{RailBlockPartition, RailSignalInput};
pub use geometry::{piece_geometry, placed_piece_geometry};
pub(in crate::simulation) use network_access::{
    conflicting_rail_end, crossing_exists, placement_connections, rail_ends_for_placement,
    signal_binding, signal_governing_crossing,
};
pub(in crate::simulation) use pathfinding::{RailRouteOutcome, RailRouteRequest, RailRouteScratch};
pub(in crate::simulation) use signalling::RailSignalling;
pub(in crate::simulation) use types::RailGraph;

use self::blocks::build_rail_blocks;
use self::graph_builder::build_rail_graph_from_pieces;
use self::types::RailPieceInput;
use super::*;

impl Simulation {
    pub(super) fn invalidate_rail_graph(&mut self) {
        self.rails.invalidate();
        // Track just appeared or vanished, which is the only moment a train can
        // lose the rail under it. Pruning here rather than lazily in the tick
        // keeps the state valid *between* ticks too, so a world saved right
        // after a train's track was blown up still loads.
        self.prune_rolling_stock();
        // The same moment and the same reason, one step further on: a plan that
        // ran over the rail which just went is a plan a train would otherwise
        // keep driving, so it is dropped here rather than discovered later by a
        // train already committed to a dead edge.
        self.invalidate_train_routes();
        // And the same again for what a train had *booked*. A claim names a
        // block by the lowest rail in it, and track changing is exactly what
        // splits and joins blocks, so a name that survived the rebuild could
        // name a different stretch of railway than the one the train was let
        // into. Every claim is given back and re-taken on the next tick, from
        // the partition as it now is; the pass hands a train the blocks it is
        // standing in before anything else asks for them, so no train is ever
        // displaced from where it already is by the reset.
        self.release_train_block_reservations();
    }

    /// Whether placing or destroying this prototype can change rail
    /// connectivity. Track and signals both can — a signal is where the block
    /// partition is cut — and nothing else does, so nothing else pays for a
    /// rebuild.
    pub(super) fn prototype_affects_rail_graph(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        prototype.rail_piece.is_some() || prototype.entity_kind.is_rail_signal()
    }

    /// Rebuilds the rail graph and the blocks it is cut into, if placement
    /// changed since the last build.
    ///
    /// One rebuild for both, because a signal appearing changes the partition
    /// and a rail appearing changes the graph the partition is taken over: two
    /// caches with two dirty flags would be two chances for one of them to
    /// describe a world the other has moved on from.
    pub(in crate::simulation) fn ensure_rail_graph(&mut self) {
        if !self.rails.graph_dirty {
            return;
        }

        let graph = build_rail_graph_from_pieces(&self.rail_piece_inputs());
        let blocks = build_rail_blocks(&graph, &self.rail_signal_inputs());
        self.rails.replace_graph(graph, blocks);
        #[cfg(test)]
        {
            self.rails.graph_rebuilds += 1;
        }
    }

    /// Every placed rail with its geometry resolved into world space, in entity
    /// id order so the graph is a function of the world and not of iteration.
    fn rail_piece_inputs(&self) -> Vec<RailPieceInput> {
        self.entities
            .placed_entities
            .values()
            .filter_map(|placed| {
                let prototype = self.world.prototypes.entity(placed.prototype_id)?;
                Some(RailPieceInput {
                    entity_id: placed.id,
                    geometry: placed_piece_geometry(placed, prototype)?,
                })
            })
            .collect()
    }

    /// Every placed signal bound to the rail end it governs, in entity id order.
    ///
    /// Two kinds of signal are left out rather than kept as a boundary they
    /// cannot govern: one that binds to nothing, because its track was pulled up
    /// from under it, and one that binds to a point the track no longer runs the
    /// way it faces. Both cut nothing and govern nothing until track comes back
    /// beside them, which is the same thing an unbuilt signal does.
    ///
    /// The alignment is re-asked here rather than trusted from placement, because
    /// track can be mined and relaid under a signal that never moved. A signal
    /// left facing across a joint it no longer runs along would still cut the
    /// boundary while governing neither direction over it — a line silently
    /// broken by an entity that looks idle, which is worse than no signal at all.
    fn rail_signal_inputs(&self) -> Vec<RailSignalInput> {
        self.entities
            .placed_entities
            .values()
            .filter_map(|placed| {
                let prototype = self.world.prototypes.entity(placed.prototype_id)?;
                let kind = prototype.entity_kind.rail_signal_kind()?;
                let position = signal_binding(self, placed.x, placed.y)?;
                crossing_exists(self, position, placed.direction).then_some(RailSignalInput {
                    entity_id: placed.id,
                    kind,
                    position,
                    heading: placed.direction,
                })
            })
            .collect()
    }

    /// Gives back every block every train holds.
    ///
    /// Called when the rail graph is invalidated, which is the one moment a
    /// block key can stop naming the stretch of railway a train was let into.
    fn release_train_block_reservations(&mut self) {
        for train in self.rolling_stock.trains.values_mut() {
            train.reserved_blocks.clear();
        }
    }
}
