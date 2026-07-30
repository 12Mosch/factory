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

/// How far a signal reaches from the middle of its own tile to the rail end it
/// governs, in fixed-point units.
///
/// A tile and a half. The base pieces put their ends on a tile boundary along
/// the track and mid-tile across it, so the nearest end to a tile beside the
/// track is about 1.1 tiles away and the next nearest about 1.8: a tile and a
/// half separates them cleanly, which is what makes "the nearest end" an
/// unambiguous answer rather than a coin toss between two joins.
const SIGNAL_BINDING_REACH_FIXED: i64 = 3 * crate::POSITION_SCALE / 2;

/// The rail end a signal standing on `(tile_x, tile_y)` governs, or `None` when
/// no track is near enough.
///
/// The nearest end to the middle of the signal's tile, out of the ends of the
/// rails in the eight tiles around it. Looking at neighbouring tiles rather than
/// scanning the placed rails is what makes this an O(1) question — it is asked
/// once per signal on every graph rebuild, and once more on every placement
/// preview frame — and it needs no rail graph, so a preview can ask it before
/// the graph has seen the track it is about to join.
///
/// Ties go to the lower point, so two ends exactly as far away resolve the same
/// way on every machine. Nothing in the base piece set can produce such a tie;
/// the rule is here because the answer feeds the block partition, and a
/// partition that depended on iteration order would desynchronise a replay.
pub(in crate::simulation) fn signal_binding(
    sim: &Simulation,
    tile_x: WorldTileCoord,
    tile_y: WorldTileCoord,
) -> Option<RailPoint> {
    let half_tile = crate::POSITION_SCALE / 2;
    let center = RailPoint::new(
        tile_x * crate::POSITION_SCALE + half_tile,
        tile_y * crate::POSITION_SCALE + half_tile,
    );
    let reach = i128::from(SIGNAL_BINDING_REACH_FIXED);
    let mut nearest: Option<(i128, RailPoint)> = None;
    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let Some(entity_id) = sim
                .entities
                .occupancy
                .entity_at(tile_x + offset_x, tile_y + offset_y)
            else {
                continue;
            };
            let Some(geometry) = sim.rail_piece_geometry(entity_id) else {
                continue;
            };
            for end in geometry.ends() {
                let dx = i128::from(end.position.x) - i128::from(center.x);
                let dy = i128::from(end.position.y) - i128::from(center.y);
                let squared = dx * dx + dy * dy;
                if squared > reach * reach {
                    continue;
                }
                if nearest.is_none_or(|(best, point)| (squared, end.position) < (best, point)) {
                    nearest = Some((squared, end.position));
                }
            }
        }
    }

    nearest.map(|(_, point)| point)
}

/// The placed signal governing a train crossing `position` while travelling
/// `heading`, ignoring `ignored_entity_id`.
///
/// Answered by looking at the tiles a signal governing that point could possibly
/// stand on — everything within [`SIGNAL_BINDING_REACH_FIXED`] of it — rather
/// than by scanning the placed signals or asking the block partition. Placement
/// validation is the caller, and it runs on a tick boundary with the partition
/// describing the world as it was before this placement.
pub(in crate::simulation) fn signal_governing_crossing(
    sim: &Simulation,
    position: RailPoint,
    heading: Direction,
    ignored_entity_id: Option<EntityId>,
) -> Option<EntityId> {
    let (tile_x, tile_y) = position.tile();
    let span = SIGNAL_BINDING_REACH_FIXED.div_euclid(crate::POSITION_SCALE) + 1;
    for offset_y in -span..=span {
        for offset_x in -span..=span {
            let Some(entity_id) = sim
                .entities
                .occupancy
                .entity_at(tile_x + offset_x, tile_y + offset_y)
            else {
                continue;
            };
            if Some(entity_id) == ignored_entity_id {
                continue;
            }
            let Some(placed) = sim.entities.placed_entity(entity_id) else {
                continue;
            };
            if placed.direction != heading
                || sim
                    .world
                    .prototypes
                    .entity(placed.prototype_id)
                    .is_none_or(|prototype| !prototype.entity_kind.is_rail_signal())
            {
                continue;
            }
            if signal_binding(sim, placed.x, placed.y) == Some(position) {
                return Some(entity_id);
            }
        }
    }

    None
}

/// Whether the track at `position` runs the way a signal facing `heading` would
/// govern: a rail leaving that point that way, and a rail entering it from the
/// other side.
///
/// The one statement of what a signal needs from the track under it, asked both
/// when a signal is placed and again whenever the block partition is rebuilt.
/// Both, because the answer can change without the signal moving: placement
/// settles it for the track that was there, and track can be mined and relaid
/// underneath afterwards.
///
/// Needs no rail graph — it reads the placed rails' own geometry — so a placement
/// preview can ask it before the graph has seen the track it is about to join.
pub(in crate::simulation) fn crossing_exists(
    sim: &Simulation,
    position: RailPoint,
    heading: Direction,
) -> bool {
    // A train travelling `heading` leaves the rail whose end here faces that way
    // and enters the one whose end here faces back, because joined ends oppose.
    rail_end_at(sim, position, heading).is_some()
        && rail_end_at(sim, position, heading.opposite()).is_some()
}

/// The placed rail with an end exactly at `position` facing `heading`.
///
/// Looks only at the tiles such a piece must occupy and confirms the candidate
/// against its geometry — no scan over the placed rails, and no dependence on
/// the rail graph, which is what lets a placement preview answer this before the
/// graph has been rebuilt.
fn rail_end_at(sim: &Simulation, position: RailPoint, heading: Direction) -> Option<EntityId> {
    candidate_tiles(position, heading)
        .into_iter()
        .flatten()
        .find_map(|(tile_x, tile_y)| {
            let entity_id = sim.entities.occupancy.entity_at(tile_x, tile_y)?;
            let geometry = sim.rail_piece_geometry(entity_id)?;
            geometry
                .ends()
                .iter()
                .any(|end| end.position == position && end.heading == heading)
                .then_some(entity_id)
        })
}

/// The tiles a piece with an end at `position` facing `heading` can occupy at
/// that end.
///
/// Its body lies on the side opposite the heading, so stepping one unit back
/// along the heading lands inside the footprint on *that* axis. On the other
/// axis the end keeps `position`'s own coordinate, and when that falls exactly
/// on a tile boundary — an end at a footprint corner — the body may sit on
/// either side of the boundary. Both tiles are then candidates; assuming the
/// first would let a corner end hide from the connection preview and from the
/// duplicate-track check.
fn candidate_tiles(position: RailPoint, heading: Direction) -> [Option<(i64, i64)>; 2] {
    let (step_x, step_y) = heading.tile_step();
    let inside = RailPoint::new(position.x - step_x, position.y - step_y);
    let on_boundary = |coordinate: i64| coordinate.rem_euclid(crate::POSITION_SCALE) == 0;

    let across = if step_x == 0 && on_boundary(position.x) {
        Some(RailPoint::new(inside.x - 1, inside.y).tile())
    } else if step_y == 0 && on_boundary(position.y) {
        Some(RailPoint::new(inside.x, inside.y - 1).tile())
    } else {
        None
    };

    [Some(inside.tile()), across]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiles(position: RailPoint, heading: Direction) -> Vec<(i64, i64)> {
        candidate_tiles(position, heading)
            .into_iter()
            .flatten()
            .collect()
    }

    /// The base pieces put their ends mid-tile across the direction of travel,
    /// so the body can only be on one side and one tile answers the question.
    #[test]
    fn an_end_between_tile_boundaries_has_a_single_candidate() {
        // A straight's north end: mid-tile in x, on a row boundary in y. The
        // piece reaching it from the north has its body in the row above.
        assert_eq!(
            tiles(RailPoint::new(512, 2_048), Direction::South),
            vec![(0, 2)]
        );
        assert_eq!(
            tiles(RailPoint::new(512, 2_048), Direction::North),
            vec![(0, 1)]
        );
    }

    /// An end at a footprint corner is on a boundary in both axes, so the body
    /// may lie on either side of the one it does not travel along. Missing the
    /// second tile would report such an end as unconnected.
    #[test]
    fn an_end_on_a_corner_has_a_candidate_either_side_of_it() {
        assert_eq!(
            tiles(RailPoint::new(0, 2_048), Direction::West),
            vec![(0, 2), (0, 1)]
        );
        assert_eq!(
            tiles(RailPoint::new(1_024, 1_024), Direction::North),
            vec![(1, 0), (0, 0)]
        );
    }
}
