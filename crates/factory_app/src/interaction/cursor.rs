use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::constants::TILE_SIZE;
use crate::rendering::manual_mining::CursorTileHighlight;

pub(crate) type CursorCameraFilter = (With<Camera2d>, Without<CursorTileHighlight>);

pub(crate) fn cursor_tile_from_window(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &Transform), CursorCameraFilter>,
) -> Option<(i32, i32)> {
    windows
        .single()
        .ok()
        .and_then(Window::cursor_position)
        .and_then(|cursor_position| {
            let (camera, camera_transform) = cameras.single().ok()?;
            let camera_global = GlobalTransform::from(*camera_transform);
            camera
                .viewport_to_world_2d(&camera_global, cursor_position)
                .ok()
        })
        .map(world_position_to_tile_coord)
}

pub fn world_position_to_tile_coord(world_position: Vec2) -> (i32, i32) {
    (
        (world_position.x / TILE_SIZE).floor() as i32,
        (world_position.y / TILE_SIZE).floor() as i32,
    )
}
