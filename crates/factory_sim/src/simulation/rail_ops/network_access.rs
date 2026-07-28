use factory_data::EntityPrototypeId;

use crate::rail::{
    RailConnectionPreview, RailEnd, RailNetworkSnapshot, RailPieceGeometry, RailPoint,
};
use crate::simulation::*;

use super::geometry::{footprint_piece_geometry, placed_piece_geometry};

impl Simulation {
    /// The rail networks the placed track forms.
    ///
    /// Derived state: the graph is rebuilt during the tick after a placement
    /// change, so a caller that placed a rail this frame sees the new network
    /// on the next tick, the same as every other network.
    pub fn rail_networks(&self) -> &[RailNetworkSnapshot] {
        debug_assert!(
            !self.rails.graph_dirty,
            "rail graph must be ensured before querying networks"
        );
        &self.rails.graph.networks
    }

    pub fn rail_network_id_for_entity(&self, entity_id: EntityId) -> Option<u32> {
        debug_assert!(
            !self.rails.graph_dirty,
            "rail graph must be ensured before querying network ids"
        );
        self.rails
            .graph
            .edge_for_entity(entity_id)
            .map(|edge| edge.network_id)
    }

    /// World-space travel geometry of a placed rail, or `None` when the entity
    /// is not track. This is the geometry the renderer draws.
    pub fn rail_piece_geometry(&self, entity_id: EntityId) -> Option<RailPieceGeometry> {
        let placed = self.entities.placed_entity(entity_id)?;
        let prototype = self.world.prototypes.entity(placed.prototype_id)?;
        placed_piece_geometry(placed, prototype)
    }

    /// The rail joined at each end of a placed rail, in the order
    /// [`RailPieceGeometry::ends`] reports them.
    pub fn rail_piece_connections(&self, entity_id: EntityId) -> [Option<EntityId>; 2] {
        debug_assert!(
            !self.rails.graph_dirty,
            "rail graph must be ensured before querying connections"
        );
        let Some(edge) = self.rails.graph.edge_for_entity(entity_id) else {
            return [None; 2];
        };
        [
            self.rails.graph.neighbor(edge, 0),
            self.rails.graph.neighbor(edge, 1),
        ]
    }

    #[cfg(test)]
    pub(in crate::simulation) fn rail_graph_rebuild_count(&self) -> u64 {
        self.rails.graph_rebuilds
    }
}

/// Both ends a prospective placement would have, in world space.
///
/// Reads only the catalog and the requested footprint, so placement validation
/// and the build preview can both use it before anything is placed.
pub(in crate::simulation) fn rail_ends_for_placement(
    world: &WorldSim,
    prototype_id: EntityPrototypeId,
    footprint: &EntityFootprint,
    direction: Direction,
) -> Option<[RailEnd; 2]> {
    let prototype = world.prototypes.entity(prototype_id)?;
    Some(footprint_piece_geometry(prototype, footprint, direction)?.ends())
}

/// What each end of a prospective rail placement would connect to.
pub(in crate::simulation) fn placement_connections(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    footprint: &EntityFootprint,
    direction: Direction,
) -> Vec<RailConnectionPreview> {
    let Some(ends) = rail_ends_for_placement(&sim.world, prototype_id, footprint, direction) else {
        return Vec::new();
    };

    ends.into_iter()
        .map(|end| RailConnectionPreview {
            position: end.position,
            heading: end.heading,
            // The rail that would join here faces the other way: its own end is
            // this point and its body lies on the far side of it.
            joins: rail_end_at(sim, end.position, end.heading.opposite()),
        })
        .collect()
}

/// An already-placed rail whose end sits exactly where a prospective placement
/// would put one of its own, facing the same way.
///
/// Two ends facing the same way are two pieces laid over each other rather than
/// a junction, so they must never both exist. Tile occupancy rejects this first
/// for the base piece set — a piece with an end here has its body on the same
/// side, so its footprint overlaps — but the rule belongs to the rail graph
/// rather than to a coincidence of the current geometry, and stating it here is
/// what stops a future piece from quietly breaking the invariant that an end
/// joins at most one other end.
pub(in crate::simulation) fn conflicting_rail_end(
    sim: &Simulation,
    ends: [RailEnd; 2],
    ignored_entity_id: Option<EntityId>,
) -> Option<(RailEnd, EntityId)> {
    ends.into_iter().find_map(|end| {
        let entity_id = rail_end_at(sim, end.position, end.heading)?;
        (Some(entity_id) != ignored_entity_id).then_some((end, entity_id))
    })
}

/// The placed rail with an end exactly at `position` facing `heading`.
///
/// A piece with such an end has its body on the side opposite its heading, so
/// the point one unit back along the heading is inside that piece's footprint.
/// One occupancy lookup therefore finds the only candidate there can be, and its
/// geometry confirms it — no scan over the placed rails, and no dependence on
/// the rail graph, which is what lets a placement preview answer this before the
/// graph has been rebuilt.
fn rail_end_at(sim: &Simulation, position: RailPoint, heading: Direction) -> Option<EntityId> {
    let (step_x, step_y) = heading.tile_step();
    let (tile_x, tile_y) = RailPoint::new(position.x - step_x, position.y - step_y).tile();
    let entity_id = sim.entities.occupancy.entity_at(tile_x, tile_y)?;
    let geometry = sim.rail_piece_geometry(entity_id)?;

    geometry
        .ends()
        .iter()
        .any(|end| end.position == position && end.heading == heading)
        .then_some(entity_id)
}
