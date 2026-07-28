//! Rail track: the fifth connectivity network, and the first with geometry.
//!
//! Power, fluid, heat, and robot networks all ask one question — which entities
//! are joined — and answer it with membership. Rails ask the same question and
//! then a second one the others never do: *where* does the joined thing run.
//! A train is not delivered from a network the way a joule or a robot is; it
//! travels a path, and that path crosses tile interiors and, on a curve, is not
//! axis-aligned at all.
//!
//! So a rail piece carries geometry, declared once on its prototype in the same
//! fixed-point units movement uses ([`factory_data::POSITION_SCALE`], 1024 per
//! tile). Everything downstream reads that one declaration: the graph built
//! here, the placement preview, and the renderer. Nothing re-derives a path from
//! a sprite, which is what would otherwise let the drawn track and the simulated
//! track drift apart.
//!
//! Two things this module deliberately does *not* do:
//!
//! * It stores no per-entity state. A rail piece is its prototype plus its
//!   direction, so it needs no entry in the entity state registry and adds
//!   nothing to a save beyond the placed entity itself.
//! * It keeps occupancy tile-locked. A piece reserves its whole rectangular
//!   footprint, so collision, mining, blueprints, and the map keep working
//!   unchanged. The cost is that two pieces can never share a tile, so track
//!   joins end to end only.

use crate::entities::Direction;
use crate::ids::EntityId;

/// A point in fixed-point position units ([`factory_data::POSITION_SCALE`] per
/// tile). Used for both world positions and footprint-local ones; which it is
/// is stated by whatever returns it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RailPoint {
    pub x: i64,
    pub y: i64,
}

impl RailPoint {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

/// One end of a rail piece: where the track reaches, and which way a train
/// leaves the piece there.
///
/// The direction is what makes an endpoint more than a point. Two pieces whose
/// ends merely touch are not joined unless they touch *facing each other* — a
/// piece heading north out of a point and one heading east out of the same point
/// are two separate spurs, not one continuous run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailEndpoint {
    pub position: RailPoint,
    /// Direction of travel leaving the piece here.
    pub direction: Direction,
}

impl RailEndpoint {
    /// Whether a train leaving this endpoint would run straight into `other`,
    /// which is exactly the rule that joins two pieces in the graph.
    pub fn joins(self, other: Self) -> bool {
        self.position == other.position && self.direction == other.direction.opposite()
    }
}

/// The path a placed rail piece takes between its endpoints, in world space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RailCurveGeometry {
    Straight,
    /// A quarter circle; both endpoints sit `radius_fixed` from `center` along
    /// perpendicular axes.
    Arc {
        center: RailPoint,
        radius_fixed: u32,
    },
}

/// A placed rail piece's travel geometry, resolved into world space.
///
/// The two endpoints are ordered as the prototype declares them. Which is which
/// carries no meaning — track is bidirectional — but the order is stable, so an
/// endpoint has an index the graph and the presentation layer can agree on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailPieceGeometry {
    pub endpoints: [RailEndpoint; 2],
    pub curve: RailCurveGeometry,
    /// Path length in fixed-point units; the weight of this piece's graph edge.
    pub length_fixed: u32,
}

/// A connected run of track.
///
/// Networks are numbered by their lowest member entity id, the same rule power,
/// fluid, heat, and robot networks use, so the numbering is a function of the
/// world rather than of iteration order: rebuilding after an unrelated placement
/// never silently renumbers the network a player is looking at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailNetworkSnapshot {
    pub network_id: u32,
    /// Member pieces, in ascending entity id order.
    pub entities: Vec<EntityId>,
    /// Total track length in the network, in fixed-point units.
    pub total_length_fixed: u64,
}

/// One endpoint of a placed piece together with the pieces it actually joins.
///
/// More than one piece can join a single endpoint: a point where a straight and
/// a curve both continue the same run is a fork, and both continuations are
/// listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailEndpointConnections {
    pub endpoint: RailEndpoint,
    /// Joined pieces, in ascending entity id order.
    pub connected: Vec<EntityId>,
}

/// What a rail piece would connect to if it were placed as previewed.
///
/// Answered without placing anything, so the build preview can show the join
/// before the player commits to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailPlacementPreview {
    pub endpoints: [RailEndpointConnections; 2],
    pub curve: RailCurveGeometry,
    pub length_fixed: u32,
}

impl RailPlacementPreview {
    /// Whether the previewed piece would join any existing track.
    pub fn joins_existing_track(&self) -> bool {
        self.endpoints
            .iter()
            .any(|endpoint| !endpoint.connected.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(x: i64, y: i64, direction: Direction) -> RailEndpoint {
        RailEndpoint {
            position: RailPoint::new(x, y),
            direction,
        }
    }

    /// Two ends at one point that face the same way are two spurs leaving it,
    /// not one continuous run. The current piece set plus tile-locked occupancy
    /// makes that arrangement unbuildable, but the rule is what the graph and
    /// the placement preview both decide on, so it is pinned here rather than
    /// left to a piece set that happens not to exercise it.
    #[test]
    fn ends_join_only_when_they_meet_facing_each_other() {
        let north = endpoint(1024, 2048, Direction::North);

        assert!(north.joins(endpoint(1024, 2048, Direction::South)));
        assert!(!north.joins(endpoint(1024, 2048, Direction::North)));
        assert!(!north.joins(endpoint(1024, 2048, Direction::East)));
        // Same heading, one unit apart: near is not the same as touching.
        assert!(!north.joins(endpoint(1024, 2049, Direction::South)));
    }

    #[test]
    fn joining_is_symmetric() {
        let first = endpoint(0, 0, Direction::East);
        let second = endpoint(0, 0, Direction::West);

        assert_eq!(first.joins(second), second.joins(first));
        assert!(first.joins(second));
    }
}
