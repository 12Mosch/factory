//! Track visuals, drawn from the simulation's own travel geometry.
//!
//! The curve here is not an authored sprite offset: it is the piece's declared
//! path, resolved by the simulation and sampled into layers. A rail that the
//! graph joins therefore always looks joined, because both answers come from
//! the same geometry.

use bevy::prelude::*;
use factory_sim::{POSITION_SCALE, RailCurve, RailPieceGeometry, RailPoint};

use super::super::EntityVisualStyle;
use super::super::layers::VisualLayerBuilder;
use crate::constants::TILE_SIZE;
use crate::rendering::colors::{rail_ballast_color, rail_metal_color, rail_sleeper_color};

/// Samples along the path. A quarter turn is about two and a third tiles long,
/// so this spaces the stamps closer than their own width and the curve reads as
/// continuous rather than dotted.
const PATH_SAMPLES: usize = 24;
/// Every this many samples carries a sleeper.
const SLEEPER_INTERVAL: usize = 4;
/// Half the distance between the two rails, in tiles.
const RAIL_GAUGE_HALF_TILES: f32 = 0.20;

pub(super) fn rail_layers(
    builder: &mut VisualLayerBuilder,
    style: EntityVisualStyle,
    geometry: RailPieceGeometry,
) {
    let path = sample_path(style.size, geometry);

    for point in &path {
        builder.ellipse(
            Vec2::splat(TILE_SIZE * 0.80),
            *point,
            0.0,
            rail_ballast_color(),
        );
    }

    for index in (0..path.len()).step_by(SLEEPER_INTERVAL) {
        builder.ellipse(
            Vec2::splat(TILE_SIZE * 0.46),
            path[index],
            0.05,
            rail_sleeper_color(),
        );
    }

    for (index, point) in path.iter().enumerate() {
        let across = tangent_at(&path, index).perp() * (TILE_SIZE * RAIL_GAUGE_HALF_TILES);
        for offset in [across, -across] {
            builder.ellipse(
                Vec2::splat(TILE_SIZE * 0.17),
                *point + offset,
                0.10,
                rail_metal_color(),
            );
        }
    }
}

/// The piece's travel path in sprite-local pixels, from one end to the other.
fn sample_path(size: Vec2, geometry: RailPieceGeometry) -> Vec<Vec2> {
    let start = to_local_pixels(size, geometry.start.position);
    let end = to_local_pixels(size, geometry.end.position);

    match geometry.curve {
        RailCurve::Straight => (0..PATH_SAMPLES)
            .map(|index| start.lerp(end, fraction(index)))
            .collect(),
        RailCurve::QuarterArc { center } => {
            let center = to_local_pixels(size, center);
            let start_offset = start - center;
            let radius = start_offset.length();
            let start_angle = start_offset.to_angle();
            // The declared arc is the quarter turn, so the sweep is the shorter
            // of the two ways round from one end to the other.
            let sweep = shortest_sweep(start_angle, (end - center).to_angle());

            (0..PATH_SAMPLES)
                .map(|index| {
                    center + Vec2::from_angle(start_angle + sweep * fraction(index)) * radius
                })
                .collect()
        }
    }
}

fn fraction(index: usize) -> f32 {
    index as f32 / (PATH_SAMPLES - 1) as f32
}

fn shortest_sweep(from_angle: f32, to_angle: f32) -> f32 {
    let mut sweep = to_angle - from_angle;
    while sweep > std::f32::consts::PI {
        sweep -= std::f32::consts::TAU;
    }
    while sweep < -std::f32::consts::PI {
        sweep += std::f32::consts::TAU;
    }
    sweep
}

/// Direction of travel at a sample, from its neighbours. Taking it from the
/// sampled path rather than from the curve keeps straights and arcs on one code
/// path and stays correct for any future piece shape.
fn tangent_at(path: &[Vec2], index: usize) -> Vec2 {
    let previous = path[index.saturating_sub(1)];
    let next = path[(index + 1).min(path.len() - 1)];
    (next - previous).normalize_or_zero()
}

/// Sub-tile world geometry as an offset from the sprite's center.
///
/// The sprite covers exactly the piece's footprint, so a point measured from the
/// footprint's minimum corner becomes a pixel offset by scaling into tiles and
/// re-centering. `+y` is north in both frames.
fn to_local_pixels(size: Vec2, point: RailPoint) -> Vec2 {
    let scale = POSITION_SCALE as f32;
    Vec2::new(
        point.x as f32 / scale * TILE_SIZE - size.x * 0.5,
        point.y as f32 / scale * TILE_SIZE - size.y * 0.5,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{Direction, RailEnd};

    fn straight_geometry() -> RailPieceGeometry {
        RailPieceGeometry {
            start: RailEnd {
                position: RailPoint::new(512, 0),
                heading: Direction::South,
            },
            end: RailEnd {
                position: RailPoint::new(512, 2_048),
                heading: Direction::North,
            },
            curve: RailCurve::Straight,
            length_fixed: 2_048,
        }
    }

    fn curved_geometry() -> RailPieceGeometry {
        RailPieceGeometry {
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
        }
    }

    /// The drawn path has to start and end exactly where the simulation says
    /// the piece's ends are, or track that the graph joins would look broken.
    #[test]
    fn the_drawn_path_starts_and_ends_at_the_pieces_own_ends() {
        let size = Vec2::new(TILE_SIZE, TILE_SIZE * 2.0);
        let path = sample_path(size, straight_geometry());

        assert_eq!(path.first().copied(), Some(Vec2::new(0.0, -TILE_SIZE)));
        assert_eq!(path.last().copied(), Some(Vec2::new(0.0, TILE_SIZE)));
    }

    #[test]
    fn a_curve_is_drawn_as_an_arc_of_its_declared_radius() {
        let size = Vec2::splat(TILE_SIZE * 2.0);
        let geometry = curved_geometry();
        let path = sample_path(size, geometry);
        let center = to_local_pixels(size, RailPoint::new(2_048, 0));
        let radius = geometry.radius_fixed() as f32 / POSITION_SCALE as f32 * TILE_SIZE;

        assert_eq!(path.len(), PATH_SAMPLES);
        for point in &path {
            assert!(
                ((*point - center).length() - radius).abs() < 0.01,
                "every sample should sit on the declared circle"
            );
        }
        // A quarter turn bulges away from the center, not across the chord.
        let apex = path[PATH_SAMPLES / 2];
        assert!(apex.x < path[0].x.max(path[PATH_SAMPLES - 1].x));
        assert!(apex.y > path[0].y);
    }
}
