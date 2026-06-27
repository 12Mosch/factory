use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::constants::TILE_SIZE;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::rendering::entities::entity_prototype_render_style;
use crate::rendering::transforms::{entity_translation, tile_translation};
use crate::resources::{BuildPlacementState, SimResource};

#[derive(Component)]
pub(crate) struct BuildPreviewSprite;

#[derive(Component)]
pub(crate) struct BuildPreviewFootprintTile;

pub(crate) fn spawn_build_preview(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(Color::srgba(0.75, 1.0, 0.72, 0.35), Vec2::splat(TILE_SIZE)),
        Transform::from_xyz(0.0, 0.0, 20.0),
        Visibility::Hidden,
        BuildPreviewSprite,
    ));
}

pub(crate) fn update_build_preview(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    mut preview: Query<(&mut Transform, &mut Sprite, &mut Visibility), With<BuildPreviewSprite>>,
    footprint_tiles: Query<Entity, With<BuildPreviewFootprintTile>>,
) {
    for entity in &footprint_tiles {
        commands.entity(entity).despawn();
    }

    let Ok((mut transform, mut sprite, mut visibility)) = preview.single_mut() else {
        return;
    };

    let Some(selection) = build_state.selected else {
        *visibility = Visibility::Hidden;
        return;
    };
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        *visibility = Visibility::Hidden;
        return;
    };

    let valid = sim
        .sim
        .can_place_entity_from_player_inventory(
            selection.prototype_id,
            selection.item_id,
            x,
            y,
            build_state.direction,
        )
        .is_ok();
    let Ok(footprint) =
        sim.sim
            .world()
            .entity_footprint(selection.prototype_id, x, y, build_state.direction)
    else {
        *visibility = Visibility::Hidden;
        return;
    };

    let (_, size) = entity_prototype_render_style(
        sim.sim.catalog(),
        selection.prototype_id,
        build_state.direction,
    )
    .unwrap_or((Color::WHITE, Vec2::splat(TILE_SIZE)));

    transform.translation = entity_translation(&footprint, 20.0);
    sprite.custom_size = Some(size);
    sprite.color = if valid {
        Color::srgba(0.78, 1.0, 0.72, 0.42)
    } else {
        Color::srgba(1.0, 0.20, 0.16, 0.42)
    };
    *visibility = Visibility::Visible;

    let footprint_color = if valid {
        Color::srgba(0.55, 1.0, 0.50, 0.20)
    } else {
        Color::srgba(1.0, 0.10, 0.08, 0.24)
    };
    for (tile_x, tile_y) in footprint.tiles() {
        commands.spawn((
            Sprite::from_color(footprint_color, Vec2::splat(TILE_SIZE - 1.0)),
            Transform::from_translation(tile_translation(tile_x, tile_y, 19.0)),
            BuildPreviewFootprintTile,
        ));
    }
}
