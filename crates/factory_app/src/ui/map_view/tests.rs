use bevy::prelude::*;

use crate::map::resources::MapTextureBounds;

use super::layout::{
    MINIMAP_VIEW_TILES, fullscreen_map_display_size, map_point_for_world_position,
    minimap_crop_bounds, texture_rect_for_world_bounds,
};

#[test]
fn minimap_crop_stays_local_when_world_is_large() {
    let map = MapTextureBounds {
        min_x: -256,
        min_y: -256,
        width: 512,
        height: 512,
    };

    let crop = minimap_crop_bounds(map, Vec2::new(0.5, 0.5));

    assert_eq!(crop.width, MINIMAP_VIEW_TILES);
    assert_eq!(crop.height, MINIMAP_VIEW_TILES);
    assert_eq!(crop.min_x, -64);
    assert_eq!(crop.min_y, -64);
}

#[test]
fn minimap_texture_rect_flips_world_y_to_image_space() {
    let map = MapTextureBounds {
        min_x: -64,
        min_y: -64,
        width: 256,
        height: 256,
    };
    let crop = MapTextureBounds {
        min_x: -32,
        min_y: 16,
        width: 128,
        height: 128,
    };

    let rect = texture_rect_for_world_bounds(map, crop);

    assert_eq!(rect.min, Vec2::new(32.0, 48.0));
    assert_eq!(rect.max, Vec2::new(160.0, 176.0));
}

#[test]
fn map_overlay_point_flips_world_y_to_ui_space() {
    let crop = MapTextureBounds {
        min_x: -32,
        min_y: 16,
        width: 128,
        height: 64,
    };

    let point = map_point_for_world_position(crop, Vec2::new(256.0, 128.0), Vec2::new(32.0, 64.0))
        .expect("point should be inside crop");

    assert_eq!(point, Vec2::new(128.0, 32.0));
}

#[test]
fn fullscreen_map_display_size_keeps_square_crop_square() {
    let crop = MapTextureBounds {
        min_x: -256,
        min_y: -256,
        width: 512,
        height: 512,
    };

    let display_size = fullscreen_map_display_size(Vec2::new(980.0, 860.0), crop);

    assert_eq!(display_size, Vec2::splat(860.0));
}

#[test]
fn fullscreen_map_display_size_fits_wide_crop_inside_available_area() {
    let crop = MapTextureBounds {
        min_x: 0,
        min_y: 0,
        width: 300,
        height: 100,
    };

    let display_size = fullscreen_map_display_size(Vec2::new(980.0, 860.0), crop);

    assert_eq!(display_size, Vec2::new(980.0, 980.0 / 3.0));
}
