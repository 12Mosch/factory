//! Resolving declared rail geometry into world space.
//!
//! A prototype declares one unrotated path; a placed piece needs that path
//! turned by its direction and offset to its footprint. Doing it once here is
//! what lets the graph, the placement preview, and the renderer all be looking
//! at the same track.

use factory_data::{EntityPrototype, POSITION_SCALE, RailCurve, RailPrototype};

use crate::rails::{RailCurveGeometry, RailEndpoint, RailPieceGeometry, RailPoint};
use crate::simulation::edge_geometry::{
    rotate_direction_clockwise, rotate_direction_counter_clockwise, rotate_local_direction,
    rotate_local_point,
};
use crate::simulation::{Direction, EntityFootprint, PlacedEntity, WorldTileCoord};

/// Travel geometry of a piece placed at `(x, y)` facing `direction`, in world
/// fixed-point units.
///
/// Returns `None` for prototypes that are not rail, which is what makes this
/// safe to call for any prototype the placement path happens to hold.
pub(in crate::simulation) fn placed_rail_geometry(
    prototype: &EntityPrototype,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
) -> Option<RailPieceGeometry> {
    let rail = prototype.rail.as_ref()?;
    let footprint = EntityFootprint::from_size(x, y, prototype.size.x, prototype.size.y, direction);
    Some(resolve_rail_geometry(
        rail,
        prototype.size.x,
        prototype.size.y,
        direction,
        footprint.x * i64::from(POSITION_SCALE),
        footprint.y * i64::from(POSITION_SCALE),
    ))
}

/// Travel geometry of an already placed piece.
pub(in crate::simulation) fn rail_geometry_for_placed(
    placed: &PlacedEntity,
    prototype: &EntityPrototype,
) -> Option<RailPieceGeometry> {
    let rail = prototype.rail.as_ref()?;
    Some(resolve_rail_geometry(
        rail,
        prototype.size.x,
        prototype.size.y,
        placed.direction,
        placed.footprint.x * i64::from(POSITION_SCALE),
        placed.footprint.y * i64::from(POSITION_SCALE),
    ))
}

/// Travel geometry relative to the footprint's minimum corner.
///
/// The renderer wants this rather than the world-space form: a sprite is shared
/// by every piece of one prototype and direction, so it must not depend on where
/// the piece sits.
pub fn rail_geometry_in_footprint(
    prototype: &EntityPrototype,
    direction: Direction,
) -> Option<RailPieceGeometry> {
    let rail = prototype.rail.as_ref()?;
    Some(resolve_rail_geometry(
        rail,
        prototype.size.x,
        prototype.size.y,
        direction,
        0,
        0,
    ))
}

fn resolve_rail_geometry(
    rail: &RailPrototype,
    width_tiles: i32,
    height_tiles: i32,
    direction: Direction,
    origin_x: i64,
    origin_y: i64,
) -> RailPieceGeometry {
    let width = i64::from(width_tiles) * i64::from(POSITION_SCALE);
    let height = i64::from(height_tiles) * i64::from(POSITION_SCALE);
    let place = |x: i32, y: i32| {
        let (rotated_x, rotated_y) =
            rotate_local_point(i64::from(x), i64::from(y), width, height, direction);
        RailPoint::new(origin_x + rotated_x, origin_y + rotated_y)
    };

    let local_directions = local_outward_directions(rail);
    RailPieceGeometry {
        endpoints: [
            RailEndpoint {
                position: place(rail.entry.x, rail.entry.y),
                direction: rotate_local_direction(local_directions[0], direction),
            },
            RailEndpoint {
                position: place(rail.exit.x, rail.exit.y),
                direction: rotate_local_direction(local_directions[1], direction),
            },
        ],
        curve: match rail.curve {
            RailCurve::Straight => RailCurveGeometry::Straight,
            RailCurve::Arc {
                center,
                radius_fixed,
            } => RailCurveGeometry::Arc {
                center: place(center.x, center.y),
                radius_fixed,
            },
        },
        length_fixed: rail.length_fixed,
    }
}

/// Which way a train leaves each end of an unrotated piece.
///
/// Derived rather than declared so the headings can never contradict the path:
/// a straight leaves along itself, and an arc leaves along the tangent pointing
/// away from its other end.
fn local_outward_directions(rail: &RailPrototype) -> [Direction; 2] {
    match rail.curve {
        RailCurve::Straight => {
            let along = axis_direction(rail.exit.x - rail.entry.x, rail.exit.y - rail.entry.y)
                .expect("validated straight rail has axis-aligned endpoints");
            [along.opposite(), along]
        }
        RailCurve::Arc { center, .. } => {
            let entry_radius = axis_direction(rail.entry.x - center.x, rail.entry.y - center.y)
                .expect("validated arc endpoint lies on an axis from the centre");
            let exit_radius = axis_direction(rail.exit.x - center.x, rail.exit.y - center.y)
                .expect("validated arc endpoint lies on an axis from the centre");
            [
                outward_arc_direction(entry_radius, exit_radius),
                outward_arc_direction(exit_radius, entry_radius),
            ]
        }
    }
}

/// The tangent at the arc endpoint whose radius points along `radius`, headed
/// away from the endpoint at `other_radius`.
///
/// The arc sweeps from one radius to the other the short way round. Travelling
/// that way is the direction that turns `radius` towards `other_radius`, so
/// *leaving* the piece at this end is the opposite turn.
fn outward_arc_direction(radius: Direction, other_radius: Direction) -> Direction {
    if rotate_direction_counter_clockwise(radius) == other_radius {
        rotate_direction_clockwise(radius)
    } else {
        rotate_direction_counter_clockwise(radius)
    }
}

/// The cardinal direction of an axis-aligned, non-zero vector.
fn axis_direction(dx: i32, dy: i32) -> Option<Direction> {
    match (dx, dy) {
        (0, 0) => None,
        (0, dy) if dy > 0 => Some(Direction::North),
        (0, _) => Some(Direction::South),
        (dx, 0) if dx > 0 => Some(Direction::East),
        (_, 0) => Some(Direction::West),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::{PrototypeCatalog, entity_prototype_id_by_name};

    fn rail_prototype<'a>(catalog: &'a PrototypeCatalog, name: &str) -> &'a EntityPrototype {
        catalog
            .entity(entity_prototype_id_by_name(catalog, name))
            .expect("base catalog defines the rail prototype")
    }

    #[test]
    fn straight_rail_runs_down_the_middle_of_its_column() {
        let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        let straight = rail_prototype(&catalog, "rail_straight");

        let geometry = placed_rail_geometry(straight, 10, 20, Direction::North)
            .expect("straight rail has geometry");

        assert_eq!(geometry.curve, RailCurveGeometry::Straight);
        assert_eq!(geometry.length_fixed, 2048);
        assert_eq!(
            geometry.endpoints[0],
            RailEndpoint {
                position: RailPoint::new(10 * 1024 + 512, 20 * 1024),
                direction: Direction::South,
            }
        );
        assert_eq!(
            geometry.endpoints[1],
            RailEndpoint {
                position: RailPoint::new(10 * 1024 + 512, 22 * 1024),
                direction: Direction::North,
            }
        );
    }

    #[test]
    fn rotating_a_straight_rail_puts_it_across_its_row() {
        let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        let straight = rail_prototype(&catalog, "rail_straight");

        let geometry = placed_rail_geometry(straight, 10, 20, Direction::East)
            .expect("straight rail has geometry");

        // Two tiles wide, one tall, running along the row's centre line.
        assert_eq!(
            geometry.endpoints[0],
            RailEndpoint {
                position: RailPoint::new(12 * 1024, 20 * 1024 + 512),
                direction: Direction::East,
            }
        );
        assert_eq!(
            geometry.endpoints[1],
            RailEndpoint {
                position: RailPoint::new(10 * 1024, 20 * 1024 + 512),
                direction: Direction::West,
            }
        );
    }

    #[test]
    fn curved_rail_turns_a_vertical_run_into_a_horizontal_one() {
        let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        let curved = rail_prototype(&catalog, "rail_curved");

        let geometry =
            placed_rail_geometry(curved, 0, 0, Direction::North).expect("curved rail has geometry");

        assert_eq!(
            geometry.curve,
            RailCurveGeometry::Arc {
                center: RailPoint::new(2048, 0),
                radius_fixed: 1536,
            }
        );
        assert_eq!(
            geometry.endpoints[0],
            RailEndpoint {
                position: RailPoint::new(512, 0),
                direction: Direction::South,
            }
        );
        assert_eq!(
            geometry.endpoints[1],
            RailEndpoint {
                position: RailPoint::new(2048, 1536),
                direction: Direction::East,
            }
        );
    }

    /// A curve's ends have to land exactly where the straights that continue the
    /// run put theirs, or no amount of graph logic will ever join them.
    #[test]
    fn a_curve_meets_the_straights_that_continue_its_run() {
        let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        let straight = rail_prototype(&catalog, "rail_straight");
        let curved = rail_prototype(&catalog, "rail_curved");

        let curve =
            placed_rail_geometry(curved, 0, 0, Direction::North).expect("curved rail has geometry");
        // The vertical straight ending where the curve begins.
        let below = placed_rail_geometry(straight, 0, -2, Direction::North)
            .expect("straight rail has geometry");
        // The horizontal straight starting where the curve ends.
        let beside = placed_rail_geometry(straight, 2, 1, Direction::East)
            .expect("straight rail has geometry");

        assert!(curve.endpoints[0].joins(below.endpoints[1]));
        assert!(curve.endpoints[1].joins(beside.endpoints[1]));
    }

    /// Every rotation of a piece has to stay inside the tiles it reserves, or
    /// tile-locked occupancy would stop meaning what it claims to.
    #[test]
    fn every_rotation_stays_inside_its_footprint() {
        let catalog = PrototypeCatalog::load_base().expect("base catalog should load");

        for name in ["rail_straight", "rail_curved"] {
            let prototype = rail_prototype(&catalog, name);
            for direction in Direction::ALL {
                let footprint =
                    EntityFootprint::from_size(0, 0, prototype.size.x, prototype.size.y, direction);
                let geometry = rail_geometry_in_footprint(prototype, direction)
                    .expect("rail prototype has geometry");
                let width = i64::from(footprint.width) * 1024;
                let height = i64::from(footprint.height) * 1024;

                for endpoint in geometry.endpoints {
                    assert!(
                        (0..=width).contains(&endpoint.position.x)
                            && (0..=height).contains(&endpoint.position.y),
                        "{name} facing {direction:?} reaches outside its footprint"
                    );
                }
            }
        }
    }
}
