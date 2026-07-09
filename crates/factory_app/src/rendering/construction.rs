use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{EntityFootprint, EntityId, GhostId};
use std::collections::HashSet;

use crate::build::resources::{PlannerState, PlannerTool};
use crate::constants::TILE_SIZE;
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::map::resources::VisibleChunks;
use crate::rendering::entities::entity_prototype_visual_style;
use crate::rendering::transforms::entity_translation;
use crate::rendering::visuals::VisualAssets;
use crate::resources::SimResource;

/// Tint multiplied over the entity visual for planned (ghost) buildings.
const GHOST_TINT: Color = Color::srgba(0.62, 0.80, 1.0, 0.55);
/// Overlay drawn across entities marked for deconstruction.
const DECONSTRUCTION_OVERLAY_COLOR: Color = Color::srgba(1.0, 0.18, 0.12, 0.34);

const GHOST_Z: f32 = 2.8;
const DECONSTRUCTION_OVERLAY_Z: f32 = 8.0;
const PASTE_PREVIEW_Z: f32 = 18.0;
const SELECTION_RECT_Z: f32 = 22.0;

#[derive(Component)]
pub(crate) struct GhostSprite {
    ghost_id: GhostId,
}

#[derive(Component)]
pub(crate) struct DeconstructionMarkOverlay {
    entity_id: EntityId,
}

#[derive(Component)]
pub(crate) struct PlannerSelectionRect;

#[derive(Component)]
pub(crate) struct PastePreviewSprite;

/// Last-synced revisions for the ghost and deconstruction overlays. All
/// construction changes bump the entity topology revision, so the two
/// revisions fully describe when a resync is needed.
#[derive(Resource, Default)]
pub(crate) struct ConstructionRenderState {
    synced: Option<(u64, u64)>,
}

pub(crate) fn spawn_planner_selection_rect(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(Color::srgba(0.9, 0.9, 0.9, 0.15), Vec2::splat(TILE_SIZE)),
        Transform::from_xyz(0.0, 0.0, SELECTION_RECT_Z),
        Visibility::Hidden,
        PlannerSelectionRect,
    ));
}

type GhostSpriteQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GhostSprite,
        &'static mut Transform,
        &'static mut Sprite,
    ),
    Without<DeconstructionMarkOverlay>,
>;
type DeconstructionOverlayQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DeconstructionMarkOverlay,
        &'static mut Transform,
        &'static mut Sprite,
    ),
    Without<GhostSprite>,
>;

/// Syncs translucent ghost sprites and deconstruction-mark overlays with the
/// simulation's construction state for the visible tile bounds.
pub(crate) fn sync_construction_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut render_state: ResMut<ConstructionRenderState>,
    mut visual_assets: VisualAssets,
    mut ghost_sprites: GhostSpriteQuery,
    mut overlays: DeconstructionOverlayQuery,
) {
    let revisions = (visible.revision, sim.sim.entity_topology_revision());
    if render_state.synced == Some(revisions) {
        return;
    }
    render_state.synced = Some(revisions);

    let construction = sim.sim.construction();
    let (visible_ghosts, visible_marks) = match visible.tile_bounds {
        Some(bounds) => {
            let max_x = bounds.min_x + bounds.width as i32 - 1;
            let max_y = bounds.min_y + bounds.height as i32 - 1;
            let ghosts =
                construction.ghost_ids_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y);
            let marks: HashSet<EntityId> = sim
                .sim
                .entities()
                .occupancy()
                .entity_ids_in_tile_rect(bounds.min_x, max_x, bounds.min_y, max_y)
                .into_iter()
                .filter(|entity_id| construction.is_marked_for_deconstruction(*entity_id))
                .collect();
            (ghosts.into_iter().collect::<HashSet<_>>(), marks)
        }
        None => (HashSet::new(), HashSet::new()),
    };

    let mut seen_ghosts = HashSet::new();
    for (entity, marker, mut transform, mut sprite) in &mut ghost_sprites {
        let ghost = visible_ghosts
            .contains(&marker.ghost_id)
            .then(|| construction.ghost(marker.ghost_id))
            .flatten();
        let style = ghost.and_then(|ghost| {
            entity_prototype_visual_style(sim.sim.catalog(), ghost.prototype_id, ghost.direction)
        });
        match (ghost, style) {
            (Some(ghost), Some(style)) => {
                seen_ghosts.insert(marker.ghost_id);
                transform.translation = entity_translation(&ghost.footprint, GHOST_Z);
                *sprite = visual_assets.entity_sprite(style);
                sprite.color = GHOST_TINT;
            }
            _ => commands.entity(entity).despawn(),
        }
    }
    for &ghost_id in &visible_ghosts {
        if seen_ghosts.contains(&ghost_id) {
            continue;
        }
        let Some(ghost) = construction.ghost(ghost_id) else {
            continue;
        };
        let Some(style) =
            entity_prototype_visual_style(sim.sim.catalog(), ghost.prototype_id, ghost.direction)
        else {
            continue;
        };
        let mut sprite = visual_assets.entity_sprite(style);
        sprite.color = GHOST_TINT;
        commands.spawn((
            sprite,
            Transform::from_translation(entity_translation(&ghost.footprint, GHOST_Z)),
            GhostSprite { ghost_id },
        ));
    }

    let mut seen_marks = HashSet::new();
    for (entity, marker, mut transform, mut sprite) in &mut overlays {
        let placed = visible_marks
            .contains(&marker.entity_id)
            .then(|| sim.sim.entities().placed_entity(marker.entity_id))
            .flatten();
        match placed {
            Some(placed) => {
                seen_marks.insert(marker.entity_id);
                transform.translation =
                    entity_translation(&placed.footprint, DECONSTRUCTION_OVERLAY_Z);
                sprite.custom_size = Some(footprint_size(&placed.footprint));
            }
            None => commands.entity(entity).despawn(),
        }
    }
    for &entity_id in &visible_marks {
        if seen_marks.contains(&entity_id) {
            continue;
        }
        let Some(placed) = sim.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        commands.spawn((
            Sprite::from_color(
                DECONSTRUCTION_OVERLAY_COLOR,
                footprint_size(&placed.footprint),
            ),
            Transform::from_translation(entity_translation(
                &placed.footprint,
                DECONSTRUCTION_OVERLAY_Z,
            )),
            DeconstructionMarkOverlay { entity_id },
        ));
    }
}

fn footprint_size(footprint: &EntityFootprint) -> Vec2 {
    Vec2::new(
        footprint.width as f32 * TILE_SIZE - 2.0,
        footprint.height as f32 * TILE_SIZE - 2.0,
    )
}

/// Shows the drag-selection rectangle while an area tool selection is in
/// progress.
pub(crate) fn update_planner_selection_rect(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    planner: Res<PlannerState>,
    mut rects: Query<(&mut Transform, &mut Sprite, &mut Visibility), With<PlannerSelectionRect>>,
) {
    let Ok((mut transform, mut sprite, mut visibility)) = rects.single_mut() else {
        return;
    };

    let selection = (!world_input_blocked(input_state.as_deref()))
        .then_some(planner.drag_start)
        .flatten()
        .zip(cursor_tile_from_window(&windows, &cameras));
    let Some((start, end)) = selection else {
        *visibility = Visibility::Hidden;
        return;
    };

    let min_x = start.0.min(end.0);
    let max_x = start.0.max(end.0);
    let min_y = start.1.min(end.1);
    let max_y = start.1.max(end.1);
    let width = (max_x - min_x + 1) as f32 * TILE_SIZE;
    let height = (max_y - min_y + 1) as f32 * TILE_SIZE;

    let cancelling = keyboard.as_deref().is_some_and(|keyboard| {
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
    });
    sprite.color = match planner.tool {
        PlannerTool::Deconstruct if cancelling => Color::srgba(0.95, 0.72, 0.25, 0.22),
        PlannerTool::Deconstruct => Color::srgba(1.0, 0.22, 0.16, 0.22),
        PlannerTool::CaptureBlueprint => Color::srgba(0.45, 0.95, 0.55, 0.20),
        _ => Color::srgba(0.45, 0.70, 1.0, 0.20),
    };
    sprite.custom_size = Some(Vec2::new(width, height));
    transform.translation = Vec3::new(
        min_x as f32 * TILE_SIZE + width * 0.5,
        min_y as f32 * TILE_SIZE + height * 0.5,
        SELECTION_RECT_Z,
    );
    *visibility = Visibility::Visible;
}

type PastePreviewQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Sprite,
        &'static mut Visibility,
    ),
    With<PastePreviewSprite>,
>;

#[derive(SystemParam)]
pub(crate) struct PastePreviewInputs<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    sim: Res<'w, SimResource>,
    planner: Res<'w, PlannerState>,
}

/// Draws the clipboard blueprint as translucent sprites following the cursor
/// while the paste tool is active.
pub(crate) fn update_paste_preview(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    inputs: PastePreviewInputs,
    mut visual_assets: VisualAssets,
    mut sprites: PastePreviewQuery,
) {
    let PastePreviewInputs {
        input_state,
        sim,
        planner,
    } = &inputs;
    let active = planner.tool == PlannerTool::Paste && !world_input_blocked(input_state.as_deref());
    let cursor = active
        .then(|| cursor_tile_from_window(&windows, &cameras))
        .flatten();
    let (Some((x, y)), Some(blueprint)) = (cursor, planner.clipboard.as_ref()) else {
        for (_, _, mut visibility) in &mut sprites {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    let catalog = sim.sim.catalog();
    let mut entries = blueprint.entities.iter().filter_map(|entity| {
        let style = entity_prototype_visual_style(catalog, entity.prototype_id, entity.direction)?;
        let prototype = catalog.entity(entity.prototype_id)?;
        let footprint = EntityFootprint::from_size(
            x + entity.dx,
            y + entity.dy,
            prototype.size.x,
            prototype.size.y,
            entity.direction,
        );
        Some((style, footprint))
    });

    let mut sprite_iter = sprites.iter_mut();
    for (mut transform, mut sprite, mut visibility) in &mut sprite_iter {
        match entries.next() {
            Some((style, footprint)) => {
                transform.translation = entity_translation(&footprint, PASTE_PREVIEW_Z);
                *sprite = visual_assets.entity_sprite(style);
                sprite.color = GHOST_TINT;
                *visibility = Visibility::Visible;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (style, footprint) in entries {
        let mut sprite = visual_assets.entity_sprite(style);
        sprite.color = GHOST_TINT;
        commands.spawn((
            sprite,
            Transform::from_translation(entity_translation(&footprint, PASTE_PREVIEW_Z)),
            PastePreviewSprite,
        ));
    }
}
