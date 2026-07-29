//! Rail graphs assembled by hand, for the tests of the code that builds and
//! searches them.
//!
//! The builder reads only three things off a piece — where its two ends are,
//! which way a train leaves through each of them, and how long the track between
//! them is — so a fixture states those directly instead of going through
//! placement and the catalog. That is what lets a test lay out a junction, which
//! the pieces in the catalog today cannot form.
//!
//! One relationship has to stay honest: a piece is never shorter than the
//! straight line between its ends. Route search estimates what is left with that
//! straight line, and a fixture that broke the relationship would make the
//! estimate an over-estimate and the search's answers arbitrary. Every helper
//! here keeps it, and every hand-written piece must.

use crate::entities::Direction;
use crate::ids::EntityId;
use crate::rail::{RailCurve, RailEnd, RailPieceGeometry, RailPoint};

use super::types::RailPieceInput;

/// Length of the two-tile straight the catalog declares, which is the unit most
/// of these fixtures are laid out in.
pub(super) const STRAIGHT_FIXED: i64 = 2_048;

/// Length of a quarter turn of the catalog's radius, and the length every
/// corner here is given. Longer than the 2_172 units across its own ends, the
/// way a real arc is.
pub(super) const CORNER_FIXED: i64 = 2_412;

/// A piece from `from` to `to`, which a train enters running `entry` and leaves
/// running `exit`.
///
/// The declared curve is not read by anything the graph does — the shape between
/// the ends only matters to the renderer and to travel along it — so a corner
/// here is stated as its two ends and its length, which is all a search sees of
/// it.
pub(super) fn piece(
    entity_id: u64,
    from: RailPoint,
    entry: Direction,
    to: RailPoint,
    exit: Direction,
    length_fixed: i64,
) -> RailPieceInput {
    RailPieceInput {
        entity_id: EntityId::new(entity_id),
        geometry: RailPieceGeometry {
            // A train leaving through the end it came in by travels back the way
            // it arrived, which is why the start's heading is the reverse of the
            // direction the piece is run in.
            start: RailEnd {
                position: from,
                heading: entry.opposite(),
            },
            end: RailEnd {
                position: to,
                heading: exit,
            },
            curve: RailCurve::Straight,
            length_fixed,
        },
    }
}

/// A straight of `length_fixed` running from `from` toward `heading`.
pub(super) fn straight(
    entity_id: u64,
    from: RailPoint,
    heading: Direction,
    length_fixed: i64,
) -> RailPieceInput {
    let (step_x, step_y) = heading.tile_step();
    let to = RailPoint::new(
        from.x + step_x * length_fixed,
        from.y + step_y * length_fixed,
    );
    piece(entity_id, from, heading, to, heading, length_fixed)
}

/// `count` two-tile straights joined end to end running north from `(512, 0)`,
/// numbered from `first_entity_id` along the run.
pub(super) fn straight_run(first_entity_id: u64, count: usize) -> Vec<RailPieceInput> {
    (0..count)
        .map(|index| {
            straight(
                first_entity_id + index as u64,
                RailPoint::new(512, index as i64 * STRAIGHT_FIXED),
                Direction::North,
                STRAIGHT_FIXED,
            )
        })
        .collect()
}
