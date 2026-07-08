use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::constants::TILE_SIZE;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::rendering::entities::entity_prototype_render_style;
use crate::rendering::transforms::{entity_translation, tile_translation};
use crate::build::resources::{BuildPlacementPreviewState, BuildPlacementState};
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct BuildPreviewSprite;

#[derive(Component)]
pub(crate) struct BuildPreviewFootprintTile;

type BuildPreviewQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Sprite,
        &'static mut Visibility,
    ),
    (With<BuildPreviewSprite>, Without<BuildPreviewFootprintTile>),
>;
type BuildPreviewFootprintTileQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Sprite,
        &'static mut Visibility,
    ),
    (With<BuildPreviewFootprintTile>, Without<BuildPreviewSprite>),
>;

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
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    preview_state: Res<BuildPlacementPreviewState>,
    mut preview: BuildPreviewQuery,
    mut footprint_tiles: BuildPreviewFootprintTileQuery,
) {
    let Ok((mut transform, mut sprite, mut visibility)) = preview.single_mut() else {
        return;
    };

    let Some(selection) = build_state.selected else {
        *visibility = Visibility::Hidden;
        hide_footprint_tiles(&mut footprint_tiles);
        return;
    };
    if preview_state.cursor_tile.is_none() {
        *visibility = Visibility::Hidden;
        hide_footprint_tiles(&mut footprint_tiles);
        return;
    }
    let Some(placement_preview) = preview_state.preview.as_ref() else {
        *visibility = Visibility::Hidden;
        hide_footprint_tiles(&mut footprint_tiles);
        return;
    };
    let Some(footprint) = placement_preview.footprint else {
        *visibility = Visibility::Hidden;
        hide_footprint_tiles(&mut footprint_tiles);
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
    sprite.color = if placement_preview.is_valid() {
        Color::srgba(0.78, 1.0, 0.72, 0.42)
    } else {
        Color::srgba(1.0, 0.20, 0.16, 0.42)
    };
    *visibility = Visibility::Visible;

    let footprint_tiles_to_draw = footprint.tiles();
    let mut tiles =
        Vec::with_capacity(footprint_tiles_to_draw.len() + placement_preview.issues.len());
    for tile in &footprint_tiles_to_draw {
        tiles.push((*tile, true));
    }
    for issue in &placement_preview.issues {
        let Some(tile) = issue.tile else {
            continue;
        };
        if !footprint_tiles_to_draw.contains(&tile)
            && !tiles.iter().any(|(drawn, _)| *drawn == tile)
        {
            tiles.push((tile, false));
        }
    }

    let mut updated_count = 0;
    for (
        ((tile_x, tile_y), is_footprint_tile),
        (mut tile_transform, mut tile_sprite, mut tile_visibility),
    ) in tiles.iter().copied().zip(footprint_tiles.iter_mut())
    {
        tile_transform.translation = tile_translation(tile_x, tile_y, 19.0);
        tile_sprite.color =
            preview_tile_color(placement_preview, (tile_x, tile_y), is_footprint_tile);
        tile_sprite.custom_size = Some(Vec2::splat(TILE_SIZE - 1.0));
        *tile_visibility = Visibility::Visible;
        updated_count += 1;
    }

    for (_, _, mut tile_visibility) in footprint_tiles.iter_mut().skip(updated_count) {
        *tile_visibility = Visibility::Hidden;
    }

    for ((tile_x, tile_y), is_footprint_tile) in tiles.into_iter().skip(updated_count) {
        commands.spawn((
            Sprite::from_color(
                preview_tile_color(placement_preview, (tile_x, tile_y), is_footprint_tile),
                Vec2::splat(TILE_SIZE - 1.0),
            ),
            Transform::from_translation(tile_translation(tile_x, tile_y, 19.0)),
            Visibility::Visible,
            BuildPreviewFootprintTile,
        ));
    }
}

pub(crate) fn update_build_placement_preview_state(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    mut preview_state: ResMut<BuildPlacementPreviewState>,
) {
    let Some(selection) = build_state.selected else {
        preview_state.cursor_tile = None;
        preview_state.preview = None;
        return;
    };
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        preview_state.cursor_tile = None;
        preview_state.preview = None;
        return;
    };

    preview_state.cursor_tile = Some((x, y));
    preview_state.preview = Some(factory_sim::placement::preview_from_player_inventory(
        &sim.sim,
        factory_sim::placement::PlayerPlacementRequest {
            prototype_id: selection.prototype_id,
            item_id: selection.item_id,
            x,
            y,
            direction: build_state.direction,
        },
    ));
}

fn preview_tile_color(
    preview: &factory_sim::BuildPlacementPreview,
    tile: (i32, i32),
    is_footprint_tile: bool,
) -> Color {
    if preview.issues.iter().any(|issue| issue.tile == Some(tile)) {
        if is_footprint_tile {
            Color::srgba(1.0, 0.08, 0.06, 0.34)
        } else {
            Color::srgba(1.0, 0.05, 0.04, 0.42)
        }
    } else {
        Color::srgba(0.55, 1.0, 0.50, 0.20)
    }
}

fn hide_footprint_tiles(footprint_tiles: &mut BuildPreviewFootprintTileQuery) {
    for (_, _, mut visibility) in footprint_tiles.iter_mut() {
        *visibility = Visibility::Hidden;
    }
}
