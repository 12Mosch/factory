use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{CHUNK_SIZE, ChunkCoord, EntityFootprint};

use crate::constants::TILE_SIZE;
use crate::map::resources::MapTextureBounds;

use super::components::FullMapImage;

pub const FULL_MAP_BASELINE_VIEW_HEIGHT: f32 = 256.0;
pub const FULL_MAP_MIN_ZOOM: f32 = 0.25;
pub const FULL_MAP_MAX_ZOOM: f32 = 8.0;
pub(super) const MINIMAP_VIEW_TILES: u32 = 128;

#[derive(Clone, Copy)]
pub(super) struct MapTileRect {
    pub(super) min: Vec2,
    pub(super) max: Vec2,
}

#[derive(Clone, Copy)]
pub(super) struct MapUiRect {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

pub(super) fn map_rect_for_footprint(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    footprint: EntityFootprint,
) -> Option<MapUiRect> {
    map_rect_for_world_rect(
        crop_bounds,
        image_size,
        MapTileRect {
            min: Vec2::new(footprint.x as f32, footprint.y as f32),
            max: Vec2::new(
                (footprint.x + footprint.width) as f32,
                (footprint.y + footprint.height) as f32,
            ),
        },
    )
}

pub(super) fn map_rect_for_chunk(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    coord: ChunkCoord,
) -> Option<MapUiRect> {
    let min = Vec2::new((coord.x * CHUNK_SIZE) as f32, (coord.y * CHUNK_SIZE) as f32);
    map_rect_for_world_rect(
        crop_bounds,
        image_size,
        MapTileRect {
            min,
            max: min + Vec2::splat(CHUNK_SIZE as f32),
        },
    )
}

pub(super) fn map_rect_for_world_rect(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    rect: MapTileRect,
) -> Option<MapUiRect> {
    if crop_bounds.width == 0 || crop_bounds.height == 0 {
        return None;
    }

    let crop_min = Vec2::new(crop_bounds.min_x as f32, crop_bounds.min_y as f32);
    let crop_max = crop_min + Vec2::new(crop_bounds.width as f32, crop_bounds.height as f32);
    let min_x = rect.min.x.max(crop_min.x);
    let max_x = rect.max.x.min(crop_max.x);
    let min_y = rect.min.y.max(crop_min.y);
    let max_y = rect.max.y.min(crop_max.y);
    if min_x >= max_x || min_y >= max_y {
        return None;
    }

    let left = (min_x - crop_min.x) / crop_bounds.width as f32 * image_size.x;
    let right = (max_x - crop_min.x) / crop_bounds.width as f32 * image_size.x;
    let top = (crop_max.y - max_y) / crop_bounds.height as f32 * image_size.y;
    let bottom = (crop_max.y - min_y) / crop_bounds.height as f32 * image_size.y;

    Some(MapUiRect {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

pub(super) fn minimap_crop_bounds(map_bounds: MapTextureBounds, center: Vec2) -> MapTextureBounds {
    let width = map_bounds.width.min(MINIMAP_VIEW_TILES);
    let height = map_bounds.height.min(MINIMAP_VIEW_TILES);

    MapTextureBounds {
        min_x: clamped_window_min(center.x, width, map_bounds.min_x, map_bounds.width),
        min_y: clamped_window_min(center.y, height, map_bounds.min_y, map_bounds.height),
        width,
        height,
    }
}

pub(super) fn clamped_window_min(
    center: f32,
    window_size: u32,
    bounds_min: i32,
    bounds_size: u32,
) -> i32 {
    if bounds_size <= window_size {
        return bounds_min;
    }

    let desired = (center - window_size as f32 * 0.5).floor() as i32;
    let max_min = bounds_min + bounds_size as i32 - window_size as i32;
    desired.clamp(bounds_min, max_min)
}

pub fn fullscreen_crop_bounds(
    map_bounds: MapTextureBounds,
    center_tile: Vec2,
    zoom: f32,
    image_size: Vec2,
) -> MapTextureBounds {
    if map_bounds.width == 0 || map_bounds.height == 0 {
        return map_bounds;
    }

    let (width, height) = fullscreen_view_tile_size(map_bounds, zoom, image_size);
    let center = clamp_map_center(map_bounds, center_tile, zoom, image_size);

    MapTextureBounds {
        min_x: clamped_window_min(center.x, width, map_bounds.min_x, map_bounds.width),
        min_y: clamped_window_min(center.y, height, map_bounds.min_y, map_bounds.height),
        width,
        height,
    }
}

pub fn clamp_map_center(
    map_bounds: MapTextureBounds,
    center_tile: Vec2,
    zoom: f32,
    image_size: Vec2,
) -> Vec2 {
    if map_bounds.width == 0 || map_bounds.height == 0 {
        return Vec2::ZERO;
    }

    let (view_width, view_height) = fullscreen_view_tile_size(map_bounds, zoom, image_size);
    Vec2::new(
        clamp_center_axis(
            center_tile.x,
            map_bounds.min_x,
            map_bounds.width,
            view_width,
        ),
        clamp_center_axis(
            center_tile.y,
            map_bounds.min_y,
            map_bounds.height,
            view_height,
        ),
    )
}

pub(super) fn fullscreen_view_tile_size(
    map_bounds: MapTextureBounds,
    zoom: f32,
    image_size: Vec2,
) -> (u32, u32) {
    let zoom = zoom.clamp(FULL_MAP_MIN_ZOOM, FULL_MAP_MAX_ZOOM);
    let aspect = if image_size.y > 0.0 {
        (image_size.x / image_size.y).max(0.1)
    } else {
        1.0
    };
    let height =
        ((FULL_MAP_BASELINE_VIEW_HEIGHT / zoom).ceil() as u32).clamp(1, map_bounds.height.max(1));
    let width = ((height as f32 * aspect).ceil() as u32).clamp(1, map_bounds.width.max(1));
    (width, height)
}

pub(super) fn clamp_center_axis(
    center: f32,
    bounds_min: i32,
    bounds_size: u32,
    view_size: u32,
) -> f32 {
    let bounds_min = bounds_min as f32;
    let bounds_max = bounds_min + bounds_size as f32;
    if bounds_size <= view_size {
        return bounds_min + bounds_size as f32 * 0.5;
    }

    let half_view = view_size as f32 * 0.5;
    center.clamp(bounds_min + half_view, bounds_max - half_view)
}

pub fn texture_rect_for_world_bounds(
    map_bounds: MapTextureBounds,
    crop_bounds: MapTextureBounds,
) -> Rect {
    let local_x = crop_bounds.min_x - map_bounds.min_x;
    let local_y = crop_bounds.min_y - map_bounds.min_y;
    let top_y = map_bounds.height as i32 - local_y - crop_bounds.height as i32;

    Rect::new(
        local_x as f32,
        top_y as f32,
        (local_x + crop_bounds.width as i32) as f32,
        (top_y + crop_bounds.height as i32) as f32,
    )
}

pub(crate) fn fullscreen_map_image_size(window: Option<&Window>) -> Vec2 {
    let Some(window) = window else {
        return Vec2::splat(980.0);
    };
    let content_size = (window.resolution.size() - Vec2::splat(56.0)).max(Vec2::splat(1.0));
    let size = content_size * 0.84;
    Vec2::new(size.x.clamp(1.0, 980.0), size.y.clamp(1.0, 980.0))
}

pub(crate) fn fullscreen_map_display_size(
    available_size: Vec2,
    crop_bounds: MapTextureBounds,
) -> Vec2 {
    if crop_bounds.width == 0 || crop_bounds.height == 0 {
        return available_size.max(Vec2::splat(1.0));
    }

    let available_size = available_size.max(Vec2::splat(1.0));
    let crop_aspect = (crop_bounds.width as f32 / crop_bounds.height as f32).max(0.1);
    let available_aspect = available_size.x / available_size.y;
    if available_aspect > crop_aspect {
        Vec2::new(available_size.y * crop_aspect, available_size.y)
    } else {
        Vec2::new(available_size.x, available_size.x / crop_aspect)
    }
}

pub(super) fn camera_tile_rect(
    cameras: &Query<(&Camera, &GlobalTransform), With<Camera2d>>,
) -> Option<MapTileRect> {
    let (camera, transform) = cameras.iter().next()?;
    let viewport_size = camera.logical_viewport_size()?;
    let first = camera.viewport_to_world_2d(transform, Vec2::ZERO).ok()?;
    let second = camera.viewport_to_world_2d(transform, viewport_size).ok()?;
    Some(MapTileRect {
        min: Vec2::new(first.x.min(second.x), first.y.min(second.y)) / TILE_SIZE,
        max: Vec2::new(first.x.max(second.x), first.y.max(second.y)) / TILE_SIZE,
    })
}

pub(super) fn fullscreen_cursor_chunk(
    crop_bounds: MapTextureBounds,
    windows: &Query<&Window, With<PrimaryWindow>>,
    image_layout: &Query<(&ComputedNode, &UiGlobalTransform), With<FullMapImage>>,
) -> Option<ChunkCoord> {
    let cursor_position = windows.single().ok()?.physical_cursor_position()?;
    let crop_max_y = crop_bounds.min_y as f32 + crop_bounds.height as f32;

    for (node, transform) in image_layout {
        let size = node.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            continue;
        }
        let local_position = transform.try_inverse()?.transform_point2(cursor_position);
        let half_size = size * 0.5;
        if local_position.x < -half_size.x
            || local_position.x > half_size.x
            || local_position.y < -half_size.y
            || local_position.y > half_size.y
        {
            continue;
        }

        let normalized = (local_position + half_size) / size;
        let tile_x = crop_bounds.min_x as f32 + normalized.x * crop_bounds.width as f32;
        let tile_y = crop_max_y - normalized.y * crop_bounds.height as f32;
        return Some(ChunkCoord {
            x: (tile_x.floor() as i32).div_euclid(CHUNK_SIZE),
            y: (tile_y.floor() as i32).div_euclid(CHUNK_SIZE),
        });
    }

    None
}

pub(super) fn map_point_for_world_position(
    crop_bounds: MapTextureBounds,
    image_size: Vec2,
    position: Vec2,
) -> Option<Vec2> {
    if crop_bounds.width == 0 || crop_bounds.height == 0 {
        return None;
    }

    let crop_min_x = crop_bounds.min_x as f32;
    let crop_min_y = crop_bounds.min_y as f32;
    let crop_max_x = crop_min_x + crop_bounds.width as f32;
    let crop_max_y = crop_min_y + crop_bounds.height as f32;
    if position.x < crop_min_x
        || position.x > crop_max_x
        || position.y < crop_min_y
        || position.y > crop_max_y
    {
        return None;
    }

    Some(Vec2::new(
        (position.x - crop_min_x) / crop_bounds.width as f32 * image_size.x,
        (crop_max_y - position.y) / crop_bounds.height as f32 * image_size.y,
    ))
}
