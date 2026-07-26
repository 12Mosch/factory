//! Draws roboport construction and logistic coverage while a roboport is held
//! or open.
//!
//! Coverage squares are huge — a construction square is over a hundred tiles
//! across — so they are drawn as one translucent quad plus four thin edge quads
//! rather than per-tile sprites. Five sprites per square keeps the overlay flat
//! in cost no matter how far a roboport reaches.
//!
//! The overlay answers two different questions depending on what the player is
//! doing, and deliberately shows different things for each:
//!
//! * **Holding** a roboport: only the prospective squares at the cursor, so the
//!   question "will this reach?" is not buried under the coverage of everything
//!   already built.
//! * **Opening** a placed roboport: every member of its network, because at
//!   that point the question is what the network as a whole covers.

use bevy::prelude::*;
use factory_sim::{Simulation, TileBounds};

use crate::build::resources::{BuildPlacementPreviewState, BuildPlacementState};
use crate::constants::TILE_SIZE;
use crate::rendering::colors::{
    construction_coverage_border_color, construction_coverage_color,
    logistic_coverage_border_color, logistic_coverage_color,
};
use crate::resources::SimResource;
use crate::ui::resources::OpenContainer;

/// Below entity sprites so a roboport's own body still reads clearly, above the
/// terrain and the build-preview footprint tiles.
const COVERAGE_FILL_Z: f32 = 4.0;
const COVERAGE_BORDER_Z: f32 = 4.1;
const BORDER_THICKNESS: f32 = 2.0;

#[derive(Component)]
pub(crate) struct RoboportCoverageSprite;

/// One coverage square to draw, in world tiles.
struct CoverageSquare {
    bounds: TileBounds,
    fill: Color,
    border: Color,
}

pub(crate) fn sync_roboport_coverage_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    preview_state: Res<BuildPlacementPreviewState>,
    open_container: Res<OpenContainer>,
    existing: Query<Entity, With<RoboportCoverageSprite>>,
) {
    // Coverage is only ever a handful of squares and only while a roboport is
    // held or open, so a full respawn per frame is cheaper than tracking
    // per-square sprite identity.
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let sim = sim.read();
    let squares = held_roboport_squares(&sim, &build_state, &preview_state)
        .unwrap_or_else(|| open_roboport_network_squares(&sim, &open_container));

    for square in squares {
        spawn_square(&mut commands, &square);
    }
}

/// Squares for a roboport currently held on the build cursor, or `None` when
/// the player is not holding one.
fn held_roboport_squares(
    sim: &Simulation,
    build_state: &BuildPlacementState,
    preview_state: &BuildPlacementPreviewState,
) -> Option<Vec<CoverageSquare>> {
    let prototype_id = build_state.selected?.entity_prototype_id()?;
    let footprint = preview_state.preview.as_ref()?.footprint?;
    let (construction, logistic) =
        sim.roboport_coverage_bounds_for_footprint(prototype_id, footprint)?;
    Some(coverage_pair(construction, logistic))
}

/// Squares for every roboport in the open roboport's network.
fn open_roboport_network_squares(
    sim: &Simulation,
    open_container: &OpenContainer,
) -> Vec<CoverageSquare> {
    let Some(entity_id) = open_container.entity_id else {
        return Vec::new();
    };
    let Some(network_id) = sim.robot_network_id_for_entity(entity_id) else {
        return Vec::new();
    };
    let Some(network) = sim
        .robot_networks()
        .iter()
        .find(|network| network.network_id == network_id)
    else {
        return Vec::new();
    };

    network
        .roboports
        .iter()
        .flat_map(|roboport| coverage_pair(roboport.construction_bounds, roboport.logistic_bounds))
        .collect()
}

fn coverage_pair(construction: TileBounds, logistic: TileBounds) -> Vec<CoverageSquare> {
    vec![
        CoverageSquare {
            bounds: construction,
            fill: construction_coverage_color(),
            border: construction_coverage_border_color(),
        },
        CoverageSquare {
            bounds: logistic,
            fill: logistic_coverage_color(),
            border: logistic_coverage_border_color(),
        },
    ]
}

fn spawn_square(commands: &mut Commands, square: &CoverageSquare) {
    // Bounds are inclusive tile indices, so the drawn rectangle spans one tile
    // more than the coordinate difference.
    let width = (square.bounds.max_x - square.bounds.min_x + 1) as f32 * TILE_SIZE;
    let height = (square.bounds.max_y - square.bounds.min_y + 1) as f32 * TILE_SIZE;
    let center = Vec2::new(
        square.bounds.min_x as f32 * TILE_SIZE + width * 0.5,
        square.bounds.min_y as f32 * TILE_SIZE + height * 0.5,
    );

    commands.spawn((
        Sprite::from_color(square.fill, Vec2::new(width, height)),
        Transform::from_translation(center.extend(COVERAGE_FILL_Z)),
        RoboportCoverageSprite,
    ));

    let half = Vec2::new(width, height) * 0.5;
    let edges = [
        (Vec2::new(width, BORDER_THICKNESS), Vec2::new(0.0, half.y)),
        (Vec2::new(width, BORDER_THICKNESS), Vec2::new(0.0, -half.y)),
        (Vec2::new(BORDER_THICKNESS, height), Vec2::new(half.x, 0.0)),
        (Vec2::new(BORDER_THICKNESS, height), Vec2::new(-half.x, 0.0)),
    ];
    for (size, offset) in edges {
        commands.spawn((
            Sprite::from_color(square.border, size),
            Transform::from_translation((center + offset).extend(COVERAGE_BORDER_Z)),
            RoboportCoverageSprite,
        ));
    }
}
