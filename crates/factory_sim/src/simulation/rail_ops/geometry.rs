//! Resolving declared rail geometry into world space.
//!
//! This is the rail counterpart of [`crate::simulation::edge_geometry`]: a
//! prototype declares its geometry once, unrotated, and one place turns that
//! into world coordinates for a placed entity. The rail graph, the placement
//! rules, and the renderer all go through here, so none of them can drift into
//! having their own idea of where the track runs.
//!
//! Rail geometry uses the placement direction convention — north is `+y`, the
//! same one belts, drills, and inserters use — rather than the tile-edge
//! mapping fluid and heat openings use. [`factory_data::RailHeading`] exists to
//! keep the two apart.

use factory_data::{
    EntityPrototype, POSITION_SCALE, RailCurvePrototype, RailHeading, RailPiecePrototype,
    RailPointPrototype,
};

use crate::entities::{Direction, EntityFootprint, PlacedEntity};
use crate::rail::{RailCurve, RailEnd, RailPieceGeometry, RailPoint};

/// Fixed-point scale shared with the prototype data. The rail path and a moving
/// entity are measured in one unit precisely because these agree.
const _: () = assert!(crate::POSITION_SCALE == POSITION_SCALE as i64);

/// A prototype's rail geometry rotated by `direction`, still measured from the
/// rotated footprint's minimum corner.
///
/// This is what the renderer draws: it has a prototype and a direction but no
/// placed entity, so a build preview shows exactly the curve a placement would
/// produce.
pub fn piece_geometry(
    prototype: &EntityPrototype,
    direction: Direction,
) -> Option<RailPieceGeometry> {
    let rail_piece = prototype.rail_piece.as_ref()?;
    let width = i64::from(prototype.size.x) * crate::POSITION_SCALE;
    let height = i64::from(prototype.size.y) * crate::POSITION_SCALE;

    Some(RailPieceGeometry {
        start: rotate_end(rail_piece.start, direction, width, height),
        end: rotate_end(rail_piece.end, direction, width, height),
        curve: rotate_curve(rail_piece, direction, width, height),
        length_fixed: rail_piece.length(),
    })
}

/// A placed rail's geometry in world coordinates.
pub fn placed_piece_geometry(
    placed: &PlacedEntity,
    prototype: &EntityPrototype,
) -> Option<RailPieceGeometry> {
    footprint_piece_geometry(prototype, &placed.footprint, placed.direction)
}

/// Rail geometry in world coordinates for a footprint that may not be placed
/// yet, which is what lets a placement preview answer what a piece *would*
/// connect to using the very same geometry a placed piece has.
pub fn footprint_piece_geometry(
    prototype: &EntityPrototype,
    footprint: &EntityFootprint,
    direction: Direction,
) -> Option<RailPieceGeometry> {
    let local = piece_geometry(prototype, direction)?;
    let origin = RailPoint::new(
        footprint.x * crate::POSITION_SCALE,
        footprint.y * crate::POSITION_SCALE,
    );

    Some(RailPieceGeometry {
        start: translate_end(local.start, origin),
        end: translate_end(local.end, origin),
        curve: match local.curve {
            RailCurve::Straight => RailCurve::Straight,
            RailCurve::QuarterArc { center } => RailCurve::QuarterArc {
                center: translate(center, origin),
            },
        },
        length_fixed: local.length_fixed,
    })
}

/// The declared heading as a placement direction. Both name the same four
/// cardinals with north at `+y`; the two types exist so rail geometry can never
/// be confused with the tile-edge sides fluids and heat use.
pub(in crate::simulation) fn heading_direction(heading: RailHeading) -> Direction {
    match heading {
        RailHeading::North => Direction::North,
        RailHeading::East => Direction::East,
        RailHeading::South => Direction::South,
        RailHeading::West => Direction::West,
    }
}

fn rotate_end(
    end: factory_data::RailEndPrototype,
    direction: Direction,
    width: i64,
    height: i64,
) -> RailEnd {
    RailEnd {
        position: rotate_point(end.position, direction, width, height),
        heading: rotate_direction(heading_direction(end.heading), direction),
    }
}

fn rotate_curve(
    rail_piece: &RailPiecePrototype,
    direction: Direction,
    width: i64,
    height: i64,
) -> RailCurve {
    match rail_piece.curve {
        RailCurvePrototype::Straight => RailCurve::Straight,
        RailCurvePrototype::QuarterArc { center } => RailCurve::QuarterArc {
            center: rotate_point(center, direction, width, height),
        },
    }
}

/// Rotates a prototype-local sub-tile point the way
/// [`crate::EntityFootprint::from_size`] rotates the footprint around it: the
/// footprint's minimum corner stays the origin and the width and height swap
/// for east and west, so a rotated point still measures from the corner of the
/// rotated footprint.
fn rotate_point(
    point: RailPointPrototype,
    direction: Direction,
    width: i64,
    height: i64,
) -> RailPoint {
    let (x, y) = (i64::from(point.x), i64::from(point.y));
    match direction {
        Direction::North => RailPoint::new(x, y),
        Direction::East => RailPoint::new(y, width - x),
        Direction::South => RailPoint::new(width - x, height - y),
        Direction::West => RailPoint::new(height - y, x),
    }
}

/// Rotates a heading by a placement direction. North is the unrotated case, so
/// each further quarter turn clockwise advances the heading by one.
fn rotate_direction(heading: Direction, direction: Direction) -> Direction {
    match direction {
        Direction::North => heading,
        Direction::East => heading.rotate_clockwise(),
        Direction::South => heading.opposite(),
        // Three quarter turns clockwise, written as the one that is cheapest to
        // read: a quarter clockwise then a half turn.
        Direction::West => heading.rotate_clockwise().opposite(),
    }
}

fn translate_end(end: RailEnd, origin: RailPoint) -> RailEnd {
    RailEnd {
        position: translate(end.position, origin),
        heading: end.heading,
    }
}

fn translate(point: RailPoint, origin: RailPoint) -> RailPoint {
    RailPoint::new(point.x + origin.x, point.y + origin.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rotating a point must land it inside the rotated footprint, and rotating
    /// four times must return the original — the property that keeps geometry
    /// and footprint in agreement for every placement.
    #[test]
    fn rotation_stays_inside_the_rotated_footprint_and_is_cyclic() {
        let (width, height) = (crate::POSITION_SCALE, 2 * crate::POSITION_SCALE);
        let point = RailPointPrototype { x: 512, y: 0 };

        for direction in Direction::ALL {
            let footprint = EntityFootprint::from_size(0, 0, 1, 2, direction);
            let rotated = rotate_point(point, direction, width, height);
            assert!(
                rotated.x >= 0
                    && rotated.x <= i64::from(footprint.width) * crate::POSITION_SCALE
                    && rotated.y >= 0
                    && rotated.y <= i64::from(footprint.height) * crate::POSITION_SCALE,
                "{direction:?} rotation left the footprint: {rotated:?}"
            );
        }

        // Four quarter turns of a square piece are the identity.
        let square = 2 * crate::POSITION_SCALE;
        let mut current = RailPointPrototype { x: 512, y: 1_536 };
        for _ in 0..4 {
            let rotated = rotate_point(current, Direction::East, square, square);
            current = RailPointPrototype {
                x: rotated.x as i32,
                y: rotated.y as i32,
            };
        }
        assert_eq!(current, RailPointPrototype { x: 512, y: 1_536 });
    }

    /// A rotated point and a rotated heading have to turn the same way, or a
    /// piece would leave through an edge it no longer touches.
    #[test]
    fn headings_turn_with_the_geometry() {
        assert_eq!(
            rotate_direction(Direction::North, Direction::East),
            Direction::East
        );
        assert_eq!(
            rotate_direction(Direction::North, Direction::South),
            Direction::South
        );
        assert_eq!(
            rotate_direction(Direction::North, Direction::West),
            Direction::West
        );

        // North (+y) rotated east becomes east (+x), matching the point map.
        let (width, height) = (crate::POSITION_SCALE, 2 * crate::POSITION_SCALE);
        let south_end = rotate_point(
            RailPointPrototype { x: 512, y: 0 },
            Direction::East,
            width,
            height,
        );
        let north_end = rotate_point(
            RailPointPrototype { x: 512, y: 2_048 },
            Direction::East,
            width,
            height,
        );
        assert!(north_end.x > south_end.x, "north must rotate onto east");
        assert_eq!(north_end.y, south_end.y);
    }
}
