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
//! * **Opening** the equipment window with a personal roboport installed: its
//!   construction square follows the player's current tile.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::EntityPrototypeId;
use factory_sim::{EntityFootprint, EntityId, Simulation, TileBounds};

use crate::build::resources::{BuildPlacementPreviewState, BuildPlacementState};
use crate::constants::TILE_SIZE;
use crate::rendering::colors::{
    construction_coverage_border_color, construction_coverage_color,
    logistic_coverage_border_color, logistic_coverage_color,
};
use crate::resources::SimResource;
use crate::ui::resources::{EquipmentWindowState, OpenContainer};

/// Below entity sprites so a roboport's own body still reads clearly, above the
/// terrain and the build-preview footprint tiles.
const COVERAGE_FILL_Z: f32 = 4.0;
const COVERAGE_BORDER_Z: f32 = 4.1;
const BORDER_THICKNESS: f32 = 2.0;

#[derive(Component)]
pub(crate) struct RoboportCoverageSprite;

/// Everything the drawn squares are a function of.
///
/// Coverage geometry only moves when the held preview moves, the open roboport
/// changes, or entities are placed or destroyed (which bumps the topology
/// revision and can add, remove, or re-network a roboport). Nothing else in a
/// frame can change a square, so an unchanged key means the existing sprites are
/// still correct.
#[derive(Clone, Copy, PartialEq, Eq)]
struct CoverageKey {
    held: Option<(EntityPrototypeId, EntityFootprint)>,
    open: Option<EntityId>,
    entity_topology_revision: u64,
    personal: Option<TileBounds>,
}

#[derive(Resource, Default)]
pub(crate) struct RoboportCoverageRenderState {
    synced: Option<CoverageKey>,
}

#[derive(SystemParam)]
pub(crate) struct RoboportCoverageInputs<'w> {
    build_state: Res<'w, BuildPlacementState>,
    preview_state: Res<'w, BuildPlacementPreviewState>,
    open_container: Res<'w, OpenContainer>,
    equipment_window: Res<'w, EquipmentWindowState>,
}

/// One coverage square to draw, in world tiles.
struct CoverageSquare {
    bounds: TileBounds,
    fill: Color,
    border: Color,
}

/// Rebuilds coverage sprites when the held roboport, open network, or moving
/// personal-roboport coverage changes.
pub(crate) fn sync_roboport_coverage_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    inputs: RoboportCoverageInputs,
    mut state: ResMut<RoboportCoverageRenderState>,
    existing: Query<Entity, With<RoboportCoverageSprite>>,
) {
    let sim = sim.read();
    let personal = inputs
        .equipment_window
        .open
        .then(|| sim.personal_roboport_coverage())
        .flatten();
    let key = CoverageKey {
        held: inputs.build_state.selected.and_then(|selection| {
            Some((
                selection.entity_prototype_id()?,
                inputs.preview_state.preview.as_ref()?.footprint?,
            ))
        }),
        open: inputs.open_container.entity_id,
        entity_topology_revision: sim.entity_topology_revision(),
        personal,
    };
    if state.synced == Some(key) {
        return;
    }
    state.synced = Some(key);

    // Coverage is at most a handful of squares and only changes on the events
    // above, so respawning the whole set on a change costs less than tracking
    // per-square sprite identity would.
    for entity in &existing {
        commands.entity(entity).despawn();
    }

    let squares = held_roboport_squares(&sim, &inputs.build_state, &inputs.preview_state)
        .unwrap_or_else(|| {
            let stationary = open_roboport_network_squares(&sim, &inputs.open_container);
            if stationary.is_empty() {
                personal.into_iter().map(personal_coverage_square).collect()
            } else {
                stationary
            }
        });

    for square in squares {
        spawn_square(&mut commands, &square);
    }
}

/// Creates the construction-only overlay used by a personal roboport.
fn personal_coverage_square(bounds: TileBounds) -> CoverageSquare {
    CoverageSquare {
        bounds,
        fill: construction_coverage_color(),
        border: construction_coverage_border_color(),
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
