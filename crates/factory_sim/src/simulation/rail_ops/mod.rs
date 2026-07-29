pub mod geometry;
mod graph_builder;
mod network_access;
mod pathfinding;
#[cfg(test)]
mod test_graphs;
mod types;

pub use geometry::{piece_geometry, placed_piece_geometry};
pub(in crate::simulation) use network_access::{
    conflicting_rail_end, placement_connections, rail_ends_for_placement,
};
pub(in crate::simulation) use pathfinding::{RailRouteOutcome, RailRouteRequest, RailRouteScratch};
pub(in crate::simulation) use types::RailGraph;

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
    }

    /// Whether placing or destroying this prototype can change rail
    /// connectivity. Only pieces that declare track geometry do, so nothing
    /// else pays for a rebuild.
    pub(super) fn prototype_affects_rail_graph(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        prototype.rail_piece.is_some()
    }

    /// Rebuilds the rail graph if placement changed since the last build.
    ///
    /// Rails carry no per-tick simulation of their own yet, so this is the
    /// whole of their tick: the graph exists for the queries placement previews,
    /// the debug overlay, and later rolling stock make of it.
    pub(in crate::simulation) fn ensure_rail_graph(&mut self) {
        if !self.rails.graph_dirty {
            return;
        }

        let graph = build_rail_graph_from_pieces(&self.rail_piece_inputs());
        self.rails.replace_graph(graph);
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
}
