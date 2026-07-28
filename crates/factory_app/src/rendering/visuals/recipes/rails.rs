//! Track drawn from the simulation's own travel geometry.
//!
//! Every other entity recipe paints a silhouette the artist chose. Rails cannot:
//! a curve that looks right but runs somewhere else is a piece of track a train
//! would leave. So this recipe takes no shape decisions of its own — it walks the
//! path [`factory_sim::rail_geometry_in_footprint`] hands it and lays ballast,
//! two rails, and sleepers along it. Change the geometry in `base.ron` and the
//! sprite follows.
//!
//! The layer primitives are axis-aligned, so a curve is drawn as a dense run of
//! small chips sampled along the path rather than as one rotated shape. That is
//! cheap here because a rail sprite is rasterized once per prototype and
//! direction and then cached.

use bevy::prelude::*;
use factory_sim::POSITION_SCALE;

use super::super::EntityVisualStyle;
use super::super::layers::VisualLayerBuilder;
use super::super::templates::{RailVisual, RailVisualCurve};
use crate::constants::TILE_SIZE;

/// Spacing between path samples, in tiles. Small enough that the chips overlap
/// on the tightest curve the piece set allows.
const SAMPLE_SPACING_TILES: f32 = 0.05;
/// Spacing between sleepers, in tiles.
const SLEEPER_SPACING_TILES: f32 = 0.4;
/// Half the distance between the two rails, in tiles.
const RAIL_GAUGE_HALF_TILES: f32 = 0.16;

pub(super) fn rail_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let Some(rail) = style.rail else {
        return;
    };

    let samples = sample_path(rail);
    let ballast = Vec2::splat(TILE_SIZE * 0.52);
    let sleeper = Vec2::splat(TILE_SIZE * 0.16);
    let rail_chip = Vec2::splat(TILE_SIZE * 0.10);
    let sleeper_stride = (SLEEPER_SPACING_TILES / SAMPLE_SPACING_TILES)
        .round()
        .max(1.0) as usize;

    for sample in &samples {
        builder.rect(
            ballast,
            sample.position,
            0.02,
            Color::srgba(0.20, 0.17, 0.13, 0.92),
        );
    }
    for sample in samples.iter().step_by(sleeper_stride) {
        builder.rect(
            sleeper,
            sample.position,
            0.05,
            Color::srgba(0.32, 0.24, 0.15, 0.95),
        );
    }
    for sample in &samples {
        let across = sample.normal * (TILE_SIZE * RAIL_GAUGE_HALF_TILES);
        builder
            .rect(rail_chip, sample.position + across, 0.09, style.base_color)
            .rect(rail_chip, sample.position - across, 0.09, style.base_color);
    }
}

/// One point on the track: where it is in sprite space and which way is across
/// the rails there.
struct PathSample {
    position: Vec2,
    normal: Vec2,
}

/// Walks the declared path from end to end at a fixed spacing.
///
/// Sprite space is centred on the footprint, while the geometry is measured from
/// the footprint's lower-left corner, so every sample is shifted by half the
/// footprint on the way out.
fn sample_path(rail: RailVisual) -> Vec<PathSample> {
    let scale = POSITION_SCALE as f32;
    let half_footprint = Vec2::new(
        rail.footprint.0 as f32 / scale * TILE_SIZE * 0.5,
        rail.footprint.1 as f32 / scale * TILE_SIZE * 0.5,
    );
    let to_sprite = |x: f32, y: f32| Vec2::new(x, y) / scale * TILE_SIZE - half_footprint;
    let entry = Vec2::new(rail.entry.0 as f32, rail.entry.1 as f32);
    let exit = Vec2::new(rail.exit.0 as f32, rail.exit.1 as f32);

    match rail.curve {
        RailVisualCurve::Straight => {
            let along = exit - entry;
            let length_tiles = along.length() / scale;
            let normal = along.perp().normalize_or_zero();
            sample_count(length_tiles)
                .map(|(_, fraction)| {
                    let point = entry + along * fraction;
                    PathSample {
                        position: to_sprite(point.x, point.y),
                        normal,
                    }
                })
                .collect()
        }
        RailVisualCurve::Arc {
            center,
            radius_fixed,
        } => {
            let center = Vec2::new(center.0 as f32, center.1 as f32);
            let radius = radius_fixed as f32;
            let start_angle = (entry - center).to_angle();
            // The arc is a quarter turn, so the shorter of the two ways round is
            // always the one the piece describes.
            let sweep = shortest_sweep(start_angle, (exit - center).to_angle());
            let length_tiles = radius.abs() * sweep.abs() / scale;
            sample_count(length_tiles)
                .map(|(_, fraction)| {
                    let angle = start_angle + sweep * fraction;
                    let radial = Vec2::from_angle(angle);
                    let point = center + radial * radius;
                    PathSample {
                        position: to_sprite(point.x, point.y),
                        normal: radial,
                    }
                })
                .collect()
        }
    }
}

/// Sample indices and their fractions along the path, endpoints included.
fn sample_count(length_tiles: f32) -> impl Iterator<Item = (usize, f32)> {
    let steps = (length_tiles / SAMPLE_SPACING_TILES).ceil().max(1.0) as usize;
    (0..=steps).map(move |step| (step, step as f32 / steps as f32))
}

/// The signed turn from `from` to `to`, taking the shorter way round.
fn shortest_sweep(from: f32, to: f32) -> f32 {
    let mut sweep = to - from;
    while sweep > std::f32::consts::PI {
        sweep -= std::f32::consts::TAU;
    }
    while sweep < -std::f32::consts::PI {
        sweep += std::f32::consts::TAU;
    }
    sweep
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::{PrototypeCatalog, entity_prototype_id_by_name};
    use factory_sim::{Direction, EntityFootprint, rail_geometry_in_footprint};

    fn rail_visual(name: &str, direction: Direction) -> RailVisual {
        let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
        let prototype = catalog
            .entity(entity_prototype_id_by_name(&catalog, name))
            .expect("base catalog defines the rail prototype");
        let footprint =
            EntityFootprint::from_size(0, 0, prototype.size.x, prototype.size.y, direction);
        RailVisual::from_geometry(
            rail_geometry_in_footprint(prototype, direction).expect("rail has geometry"),
            footprint.width,
            footprint.height,
        )
    }

    /// The drawn track has to start and end exactly where the simulation says
    /// the piece reaches, or the sprite would promise a join the graph does not
    /// make.
    #[test]
    fn sampled_path_starts_and_ends_at_the_declared_endpoints() {
        for name in ["rail_straight", "rail_curved"] {
            for direction in Direction::ALL {
                let rail = rail_visual(name, direction);
                let samples = sample_path(rail);
                let scale = POSITION_SCALE as f32;
                let half = Vec2::new(
                    rail.footprint.0 as f32 / scale * TILE_SIZE * 0.5,
                    rail.footprint.1 as f32 / scale * TILE_SIZE * 0.5,
                );
                let expected = |point: (i64, i64)| {
                    Vec2::new(point.0 as f32, point.1 as f32) / scale * TILE_SIZE - half
                };

                assert!(
                    samples[0].position.distance(expected(rail.entry)) < 1e-3,
                    "{name} facing {direction:?} does not start at its entry"
                );
                assert!(
                    samples[samples.len() - 1]
                        .position
                        .distance(expected(rail.exit))
                        < 1e-3,
                    "{name} facing {direction:?} does not end at its exit"
                );
            }
        }
    }

    /// A curve's samples must all sit on the declared arc, otherwise the drawn
    /// track would bulge away from the path a train would follow.
    #[test]
    fn curved_samples_stay_on_the_declared_arc() {
        let rail = rail_visual("rail_curved", Direction::North);
        let RailVisualCurve::Arc {
            center,
            radius_fixed,
        } = rail.curve
        else {
            panic!("the curved rail declares an arc");
        };
        let scale = POSITION_SCALE as f32;
        let half = Vec2::new(
            rail.footprint.0 as f32 / scale * TILE_SIZE * 0.5,
            rail.footprint.1 as f32 / scale * TILE_SIZE * 0.5,
        );
        let center_sprite = Vec2::new(center.0 as f32, center.1 as f32) / scale * TILE_SIZE - half;
        let expected_radius = radius_fixed as f32 / scale * TILE_SIZE;

        for sample in sample_path(rail) {
            assert!(
                (sample.position.distance(center_sprite) - expected_radius).abs() < 1e-3,
                "sample left the arc"
            );
        }
    }
}
