//! Rails: sub-tile track geometry and the graph it forms.
//!
//! A rail piece is an ordinary placed entity — it reserves whole tiles in the
//! occupancy grid, saves like any other entity, and is built through the same
//! ghost and blueprint paths. What makes it track is the geometry its prototype
//! declares: two ends, each a point and the heading a train travels when it
//! leaves through that end, and the curve between them
//! ([`factory_data::RailPiecePrototype`]).
//!
//! Two rails connect where one piece's end meets another's at the same point
//! with the opposite heading. That is a statement about travel, not about
//! footprints touching, which is why the graph can be built from geometry alone
//! and why a rail running past the side of another never joins it.
//!
//! Occupancy stays tile-locked on purpose: a curve reserves every tile of its
//! footprint, including the ones its arc only clips. Collision, mining,
//! blueprints, and the map therefore never learn about sub-tile geometry, and
//! the sub-tile part stays confined to the piece definitions and to the code in
//! this module.
//!
//! The graph itself is a derived cache. It is rebuilt from the placed rails
//! whenever placement changes — the invalidate-and-rebuild shape the power,
//! fluid, heat, and robot networks already share — and is never saved.
//!
//! Signals partition that graph into *blocks*: a block is a maximal run of
//! track joined without a signal standing between. The partition is derived from
//! the same topology cache rather than being a second graph, and it is what
//! reservation is expressed over — see
//! [`crate::simulation::rail_ops`] for the reservation rules and
//! [`RailSignalAspect`] for what a signal reports about the block it guards.

use crate::entities::Direction;
use crate::ids::EntityId;
use factory_data::RailSignalKind;
use serde::{Deserialize, Serialize};

/// A point in world sub-tile space, in fixed-point units
/// ([`crate::POSITION_SCALE`] per tile). The same units free-moving positions
/// use, so a train position and a rail path never need converting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RailPoint {
    pub x: i64,
    pub y: i64,
}

impl RailPoint {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    /// The tile this point sits in. Points on a tile boundary belong to the
    /// tile above/right of it, which is the same rounding the occupancy grid
    /// uses for positions.
    pub const fn tile(self) -> (i64, i64) {
        (
            self.x.div_euclid(crate::POSITION_SCALE),
            self.y.div_euclid(crate::POSITION_SCALE),
        )
    }
}

/// One end of a placed rail piece, resolved into world space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailEnd {
    pub position: RailPoint,
    /// Direction of travel leaving the piece here. Two ends join when they
    /// share a position and face opposite headings.
    pub heading: Direction,
}

/// The path a placed piece takes between its two ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RailCurve {
    Straight,
    /// A quarter circle around `center`, taken the short way round.
    QuarterArc {
        center: RailPoint,
    },
}

/// A placed rail piece's travel geometry in world space, and the length the
/// rail graph carries on its edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailPieceGeometry {
    pub start: RailEnd,
    pub end: RailEnd,
    pub curve: RailCurve,
    /// Travel length in fixed-point units.
    pub length_fixed: i64,
}

impl RailPieceGeometry {
    pub const fn ends(&self) -> [RailEnd; 2] {
        [self.start, self.end]
    }

    /// Turning radius in fixed-point units, or zero for a straight. Derived
    /// from the arc rather than stored, so there is one description of the
    /// curve and not two that can disagree.
    pub fn radius_fixed(&self) -> i64 {
        let RailCurve::QuarterArc { center } = self.curve else {
            return 0;
        };
        let dx = self.start.position.x - center.x;
        let dy = self.start.position.y - center.y;
        (dx * dx + dy * dy).isqrt()
    }
}

/// A connected set of rail pieces.
///
/// Derived from the placed rails, so it is rebuilt on load rather than saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RailNetworkSnapshot {
    pub network_id: u32,
    pub piece_count: usize,
    /// Distinct rail ends in the network. A run of `n` pieces joined end to end
    /// has `n + 1` nodes, so this is what tells a closed loop from an open run.
    pub node_count: usize,
    pub total_length_fixed: i64,
}

/// A block of track: a maximal run of rail joined without a signal between.
///
/// Derived from the graph and the placed signals, so it is rebuilt whenever
/// either changes rather than saved. What *is* saved is a train's claim on a
/// block, and that claim names the block by [`RailBlockSnapshot::key`] for
/// exactly that reason: an index into the partition would mean something else
/// one placement later, while the lowest entity id in a block is a name the
/// world itself carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailBlockSnapshot {
    /// The block's durable name: the lowest rail entity id it contains.
    pub key: EntityId,
    pub piece_count: usize,
    pub total_length_fixed: i64,
    /// Signals standing on this block's boundary that admit a train *into* it.
    pub entry_signal_count: usize,
}

/// What a signal says about the block beyond it.
///
/// Three states rather than the two a train actually cares about, because
/// "somebody is standing in there" and "somebody is on their way in" are
/// different things to a player reading the railway — and the same thing to a
/// train, which stops either way.
///
/// A chain signal reports the block it guards like any other signal does. What
/// makes it a chain is the reservation rule it follows, not a fourth aspect: a
/// chain signal showing [`RailSignalAspect::Clear`] may still hold a train,
/// because the signal *beyond* it could not clear.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize,
)]
pub enum RailSignalAspect {
    /// Nothing is in the block beyond and nothing has claimed it.
    #[default]
    Clear,
    /// A train holds the block beyond but has not reached it yet.
    Reserved,
    /// A train is standing in the block beyond.
    Blocked,
}

impl RailSignalAspect {
    /// What a circuit connector publishes for this aspect.
    ///
    /// Deliberately non-zero for every aspect: a network carries no difference
    /// between a channel holding zero and a channel nothing writes to, so a
    /// clear signal reading zero would be indistinguishable from no signal being
    /// wired at all.
    pub const fn circuit_value(self) -> i32 {
        match self {
            Self::Clear => 1,
            Self::Reserved => 2,
            Self::Blocked => 3,
        }
    }
}

/// A placed signal as the block partition sees it.
///
/// The point and the heading together are the signal's position on the railway:
/// a train crossing that point travelling that way is the one this signal
/// governs, and a train crossing it the other way is governed by whatever
/// signal faces the other side — or by nothing, which makes the boundary
/// impassable that way. That is what "the side it faces is significant" means
/// once the geometry is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailSignalSnapshot {
    pub entity_id: EntityId,
    pub kind: RailSignalKind,
    /// The rail end the signal stands at.
    pub position: RailPoint,
    /// The travel heading this signal governs.
    pub heading: Direction,
    /// Block a train is in as it comes up to the signal, or `None` when no
    /// track approaches it from that side.
    pub approach_block: Option<EntityId>,
    /// Block the signal admits a train into, or `None` when the track ends
    /// there.
    pub guarded_block: Option<EntityId>,
}

/// One end of a rail the player is about to place, and the rail it would join.
///
/// The build preview draws these so a player can see the connection a piece
/// would form before committing to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RailConnectionPreview {
    pub position: RailPoint,
    pub heading: Direction,
    /// The placed rail this end would join, or `None` for a free end.
    pub joins: Option<EntityId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_resolve_to_the_tile_they_sit_in() {
        assert_eq!(RailPoint::new(0, 0).tile(), (0, 0));
        assert_eq!(RailPoint::new(512, 1_536).tile(), (0, 1));
        assert_eq!(RailPoint::new(-1, -1).tile(), (-1, -1));
    }

    #[test]
    fn a_quarter_arc_reports_its_radius_and_a_straight_reports_none() {
        let arc = RailPieceGeometry {
            start: RailEnd {
                position: RailPoint::new(512, 0),
                heading: Direction::South,
            },
            end: RailEnd {
                position: RailPoint::new(2_048, 1_536),
                heading: Direction::East,
            },
            curve: RailCurve::QuarterArc {
                center: RailPoint::new(2_048, 0),
            },
            length_fixed: 2_412,
        };

        assert_eq!(arc.radius_fixed(), 1_536);
        assert_eq!(
            RailPieceGeometry {
                curve: RailCurve::Straight,
                ..arc
            }
            .radius_fixed(),
            0
        );
    }
}
