use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use std::time::Instant;

use crate::map::resources::{
    MapDetailCache, MapDisplaySettings, MapOverlayLayer, MapOverlayMarkers, MapTextureBounds,
};

use super::drawing::{MapOverlayContext, reconcile_map_overlay};
use super::layout::{
    MINIMAP_VIEW_TILES, fullscreen_map_display_size, map_point_for_world_position,
    minimap_crop_bounds, texture_rect_for_world_bounds,
};
use super::sync::map_detail_cache_key;

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

#[test]
fn overlay_reconciliation_preserves_player_entity() {
    let mut world = World::new();
    let root = world.spawn_empty().id();
    let sim = factory_sim::Simulation::new_test_world(123);
    let mut details = MapDetailCache::default();
    let settings = MapDisplaySettings::default();
    let markers = MapOverlayMarkers::default();
    let bounds = MapTextureBounds {
        min_x: -64,
        min_y: -64,
        width: 128,
        height: 128,
    };

    let mut original_player_entity = None;
    for player_position in [Vec2::ZERO, Vec2::X] {
        let changed_layers = details.changed_layers(
            root,
            map_detail_cache_key(
                bounds,
                Vec2::splat(176.0),
                (player_position, None, None),
                &sim,
                &settings,
                &markers,
            ),
        );
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        reconcile_map_overlay(
            &mut commands,
            root,
            &mut details,
            changed_layers,
            MapOverlayContext {
                crop_bounds: bounds,
                image_size: Vec2::splat(176.0),
                player_position,
                sim: &sim,
                settings: &settings,
                camera_rect: None,
                chunk_cursor: None,
                markers: &markers,
            },
        );
        queue.apply(&mut world);
        let player_entity = details.layer_entities_mut(root, MapOverlayLayer::Player)[0];
        if let Some(original) = original_player_entity {
            assert_eq!(player_entity, original);
        } else {
            original_player_entity = Some(player_entity);
        }
    }

    let player_entities = details.layer_entities_mut(root, MapOverlayLayer::Player);
    assert_eq!(player_entities.len(), 1);
    let player_entity = player_entities[0];
    let node = world
        .get::<Node>(player_entity)
        .expect("reconciled player marker should remain alive");
    assert_eq!(node.left, Val::Px(84.875));
}

#[test]
#[ignore]
fn minimap_overlay_reconcile_benchmark() {
    const WARMUP_FRAMES: usize = 30;
    const MEASUREMENT_FRAMES: usize = 300;

    let mut world = World::new();
    let root = world.spawn_empty().id();
    let mut sim = factory_sim::Simulation::new_test_world(123);
    let mut details = MapDetailCache::default();
    let settings = MapDisplaySettings::default();
    let markers = MapOverlayMarkers::default();
    let bounds = MapTextureBounds {
        min_x: -64,
        min_y: -64,
        width: 128,
        height: 128,
    };

    let reconcile =
        |world: &mut World, sim: &factory_sim::Simulation, details: &mut MapDetailCache| {
            let changed_layers = details.changed_layers(
                root,
                map_detail_cache_key(
                    bounds,
                    Vec2::splat(176.0),
                    (Vec2::ZERO, None, None),
                    sim,
                    &settings,
                    &markers,
                ),
            );
            let mut queue = CommandQueue::default();
            let mut commands = Commands::new(&mut queue, world);
            reconcile_map_overlay(
                &mut commands,
                root,
                details,
                changed_layers,
                MapOverlayContext {
                    crop_bounds: bounds,
                    image_size: Vec2::splat(176.0),
                    player_position: Vec2::ZERO,
                    sim,
                    settings: &settings,
                    camera_rect: None,
                    chunk_cursor: None,
                    markers: &markers,
                },
            );
            queue.apply(world);
        };

    for _ in 0..WARMUP_FRAMES {
        sim.tick();
        reconcile(&mut world, &sim, &mut details);
    }

    let before_entity_slots = world.entities().len();
    let start = Instant::now();
    for _ in 0..MEASUREMENT_FRAMES {
        sim.tick();
        reconcile(&mut world, &sim, &mut details);
    }
    let elapsed = start.elapsed();
    let after_entity_slots = world.entities().len();
    println!(
        "minimap_overlay_reconcile_benchmark: {MEASUREMENT_FRAMES} frames in {:.3} ms ({:.3} ms/frame), allocated entity slots {before_entity_slots} -> {after_entity_slots}",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0 / MEASUREMENT_FRAMES as f64,
    );
}
