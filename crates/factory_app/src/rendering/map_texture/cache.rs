use bevy::prelude::*;
use factory_sim::Simulation;

use crate::map::resources::{
    MapDisplaySettings, MapLayer, MapLayerTextureCache, MapTextureBounds, MapTextureCache,
    MapViewState,
};
use crate::resources::SimResource;

use super::bounds::map_texture_bounds;
use super::grid::draw_chunk_grid;
use super::pixels::pixel_offset;
use super::rasterizer::MapRasterizer;
use super::upload::upload_layer_texture;

const RESOURCE_TEXTURE_REFRESH_INTERVAL_TICKS: u64 = 15;

pub(crate) fn update_map_texture(
    sim: Res<SimResource>,
    settings: Res<MapDisplaySettings>,
    state: Res<MapViewState>,
    mut cache: ResMut<MapTextureCache>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut images) = images else {
        return;
    };

    // The surface layer also backs the minimap, so it stays fresh even while
    // the fullscreen map is closed. Other layers only update while displayed.
    let surface_cache = cache.layer_mut(MapLayer::Surface);
    update_layer_map_texture(
        &sim.sim,
        &settings,
        MapLayer::Surface,
        surface_cache,
        &mut images,
    );

    if state.open && state.selected_layer != MapLayer::Surface {
        let layer_cache = cache.layer_mut(state.selected_layer);
        update_layer_map_texture(
            &sim.sim,
            &settings,
            state.selected_layer,
            layer_cache,
            &mut images,
        );
    }
}

fn update_layer_map_texture(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
    cache: &mut MapLayerTextureCache,
    images: &mut Assets<Image>,
) {
    let revealed_revision = sim.revealed_revision();
    let debug_flags = (settings.debug_reveal_all, settings.show_chunk_grid);
    let tick_count = sim.tick_count();
    let chunk_changed = cache.last_chunk_revision != sim.world().chunk_revision();
    let resource_changed = cache.last_resource_revision != sim.world().resource_revision();
    let revealed_changed = cache.last_revealed_revision != revealed_revision;
    let debug_changed = cache.last_debug_flags != debug_flags;
    let needs_update = cache.handle.is_none()
        || chunk_changed
        || revealed_changed
        || debug_changed
        || (resource_changed && should_refresh_texture(cache.last_texture_update_tick, tick_count));

    if !needs_update {
        return;
    }

    let rasterizer = MapRasterizer::new(sim, settings, layer);
    let full_rebuild = cache.bounds.is_none() || cache.pixels.is_none() || debug_changed;
    if full_rebuild {
        let map = rasterizer.generate();
        cache.bounds = Some(map.bounds);
        cache.pixels = Some(map.data);
        refresh_painted_chunks(&rasterizer, cache);
    } else {
        update_map_pixels_incremental(&rasterizer, cache);
    }

    upload_layer_texture(cache, images);

    cache.last_chunk_revision = sim.world().chunk_revision();
    cache.last_resource_revision = sim.world().resource_revision();
    cache.last_revealed_revision = revealed_revision;
    cache.last_debug_flags = debug_flags;
    cache.last_texture_update_tick = tick_count;
}

fn should_refresh_texture(last_texture_update_tick: u64, tick_count: u64) -> bool {
    tick_count.saturating_sub(last_texture_update_tick) >= RESOURCE_TEXTURE_REFRESH_INTERVAL_TICKS
}

fn update_map_pixels_incremental(rasterizer: &MapRasterizer<'_>, cache: &mut MapLayerTextureCache) {
    let old_bounds = cache.bounds.unwrap_or_default();
    let new_bounds = map_texture_bounds(rasterizer.sim, rasterizer.settings).unwrap_or_default();
    let bounds_changed = old_bounds != new_bounds;
    if bounds_changed {
        resize_cached_pixels(cache, old_bounds, new_bounds);
        // The resize only preserves pixels inside both bounds; drop the paint
        // state of clipped chunks so they get repainted at their new position.
        cache.painted_chunks.retain(|coord, _| {
            old_bounds.contains_chunk(*coord) && new_bounds.contains_chunk(*coord)
        });
    }
    repaint_changed_chunks(rasterizer, cache);
    repaint_dirty_resource_tiles(rasterizer, cache);

    if bounds_changed && rasterizer.settings.show_chunk_grid {
        let Some(bounds) = cache.bounds else {
            return;
        };
        let Some(data) = cache.pixels.as_mut() else {
            return;
        };
        draw_chunk_grid(data, bounds);
    }
}

fn resize_cached_pixels(
    cache: &mut MapLayerTextureCache,
    old_bounds: MapTextureBounds,
    new_bounds: MapTextureBounds,
) {
    let Some(old_pixels) = cache.pixels.take() else {
        cache.bounds = Some(new_bounds);
        cache.pixels = Some(vec![
            0;
            new_bounds.width as usize
                * new_bounds.height as usize
                * 4
        ]);
        cache.painted_chunks.clear();
        return;
    };

    let mut new_pixels = vec![0; new_bounds.width as usize * new_bounds.height as usize * 4];
    let old_max_x = old_bounds.min_x + old_bounds.width as i32 - 1;
    let old_max_y = old_bounds.min_y + old_bounds.height as i32 - 1;
    let new_max_x = new_bounds.min_x + new_bounds.width as i32 - 1;
    let new_max_y = new_bounds.min_y + new_bounds.height as i32 - 1;
    let min_x = old_bounds.min_x.max(new_bounds.min_x);
    let max_x = old_max_x.min(new_max_x);
    let min_y = old_bounds.min_y.max(new_bounds.min_y);
    let max_y = old_max_y.min(new_max_y);

    if min_x <= max_x && min_y <= max_y {
        let row_len = (max_x - min_x + 1) as usize * 4;
        for world_y in min_y..=max_y {
            let old_offset = pixel_offset(old_bounds, min_x, world_y);
            let new_offset = pixel_offset(new_bounds, min_x, world_y);
            new_pixels[new_offset..new_offset + row_len]
                .copy_from_slice(&old_pixels[old_offset..old_offset + row_len]);
        }
    }

    cache.bounds = Some(new_bounds);
    cache.pixels = Some(new_pixels);
}

fn repaint_changed_chunks(rasterizer: &MapRasterizer<'_>, cache: &mut MapLayerTextureCache) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    cache
        .painted_chunks
        .retain(|coord, _| rasterizer.sim.world().chunks.contains_key(coord));

    for chunk in rasterizer.sim.world().chunks.values() {
        let state = rasterizer.chunk_paint_state(chunk.coord);
        if cache.painted_chunks.get(&chunk.coord) == Some(&state) {
            continue;
        }
        rasterizer.repaint_chunk(data, bounds, chunk.coord);
        cache.painted_chunks.insert(chunk.coord, state);
    }
}

fn repaint_dirty_resource_tiles(rasterizer: &MapRasterizer<'_>, cache: &mut MapLayerTextureCache) {
    let Some(changes) = rasterizer
        .sim
        .world()
        .resource_dirty_tiles_since(cache.last_resource_revision)
    else {
        repaint_all_chunks(rasterizer, cache);
        return;
    };
    let changes = changes.collect::<Vec<_>>();
    if changes.is_empty() {
        return;
    }

    let Some(bounds) = cache.bounds else {
        return;
    };
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    for change in changes {
        rasterizer.repaint_tile(data, bounds, change.x, change.y);
    }
}

fn repaint_all_chunks(rasterizer: &MapRasterizer<'_>, cache: &mut MapLayerTextureCache) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    cache.painted_chunks.clear();
    for chunk in rasterizer.sim.world().chunks.values() {
        rasterizer.repaint_chunk(data, bounds, chunk.coord);
        cache
            .painted_chunks
            .insert(chunk.coord, rasterizer.chunk_paint_state(chunk.coord));
    }
}

fn refresh_painted_chunks(rasterizer: &MapRasterizer<'_>, cache: &mut MapLayerTextureCache) {
    cache.painted_chunks = rasterizer
        .sim
        .world()
        .chunks
        .keys()
        .copied()
        .map(|coord| (coord, rasterizer.chunk_paint_state(coord)))
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::map_texture::generate_map_pixels_for_layer;
    use factory_sim::{CHUNK_SIZE, ChunkCoord, WorldSim};

    #[test]
    fn incremental_update_matches_full_render_after_streaming_chunk() {
        let settings_variants = [
            MapDisplaySettings::default(),
            MapDisplaySettings {
                debug_reveal_all: false,
                show_chunk_grid: true,
            },
            MapDisplaySettings {
                debug_reveal_all: true,
                show_chunk_grid: true,
            },
        ];
        for settings in settings_variants {
            assert_incremental_update_matches_full_render_after_streaming_chunk(settings);
        }
    }

    fn assert_incremental_update_matches_full_render_after_streaming_chunk(
        settings: MapDisplaySettings,
    ) {
        let layers = [MapLayer::Surface, MapLayer::Resources, MapLayer::Entities];
        let mut sim = Simulation::new_test_world(123);
        let mut caches = layers.map(|layer| {
            let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
            let mut cache = MapLayerTextureCache {
                handle: Some(Handle::default()),
                bounds: Some(initial.bounds),
                pixels: Some(initial.data),
                painted_chunks: Default::default(),
                last_chunk_revision: sim.world().chunk_revision(),
                last_resource_revision: sim.world().resource_revision(),
                last_revealed_revision: sim.revealed_revision(),
                last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
                last_texture_update_tick: sim.tick_count(),
            };
            let rasterizer = MapRasterizer::new(&sim, &settings, layer);
            refresh_painted_chunks(&rasterizer, &mut cache);
            cache
        });

        let target_chunk = ChunkCoord { x: 0, y: -9 };
        let before_chunk_revision = sim.world().chunk_revision();
        let before_revealed_revision = sim.revealed_revision();
        assert!(
            !sim.world().chunks.contains_key(&target_chunk),
            "target chunk should start outside the generated world"
        );
        let target = first_walkable_tile_in_chunk(sim.seed(), target_chunk);
        move_player_to_tile(&mut sim, target);
        sim.tick();
        assert!(
            sim.world().chunks.contains_key(&target_chunk),
            "moving to the target should stream the chunk"
        );
        assert!(
            sim.world().chunk_revision() > before_chunk_revision,
            "streaming the target chunk should advance chunk revision"
        );
        assert!(
            sim.is_chunk_revealed(target_chunk),
            "ticking at the target should reveal the streamed chunk"
        );
        assert!(
            sim.revealed_revision() != before_revealed_revision,
            "revealing new chunks should advance the revealed revision"
        );

        for (layer, cache) in layers.iter().zip(caches.iter_mut()) {
            let rasterizer = MapRasterizer::new(&sim, &settings, *layer);
            update_map_pixels_incremental(&rasterizer, cache);

            let full = generate_map_pixels_for_layer(&sim, &settings, *layer);
            assert_eq!(
                cache.bounds,
                Some(full.bounds),
                "bounds for {layer:?} with {settings:?}"
            );
            assert_eq!(
                cache.pixels.as_deref(),
                Some(full.data.as_slice()),
                "pixels for {layer:?} with {settings:?}"
            );
        }
    }

    #[test]
    #[ignore]
    fn bench_incremental_update_on_bounds_growth() {
        let mut sim = Simulation::new_test_world(123);
        for y in -15..=15 {
            for x in -15..=15 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
        let settings = MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: false,
        };

        let initial = generate_map_pixels_for_layer(&sim, &settings, MapLayer::Surface);
        let mut cache = MapLayerTextureCache {
            handle: Some(Handle::default()),
            bounds: Some(initial.bounds),
            pixels: Some(initial.data),
            painted_chunks: Default::default(),
            last_chunk_revision: sim.world().chunk_revision(),
            last_resource_revision: sim.world().resource_revision(),
            last_revealed_revision: sim.revealed_revision(),
            last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
            last_texture_update_tick: sim.tick_count(),
        };
        let rasterizer = MapRasterizer::new(&sim, &settings, MapLayer::Surface);
        refresh_painted_chunks(&rasterizer, &mut cache);

        // Frontier chunk grows the texture bounds by one chunk column.
        sim.ensure_chunk_generated(ChunkCoord { x: 16, y: 0 });

        let started = std::time::Instant::now();
        let rasterizer = MapRasterizer::new(&sim, &settings, MapLayer::Surface);
        update_map_pixels_incremental(&rasterizer, &mut cache);
        let elapsed = started.elapsed();
        println!("incremental update after bounds growth: {elapsed:?}");

        let full = generate_map_pixels_for_layer(&sim, &settings, MapLayer::Surface);
        assert_eq!(cache.bounds, Some(full.bounds));
        assert_eq!(cache.pixels.as_deref(), Some(full.data.as_slice()));
    }

    fn first_walkable_tile_in_chunk(seed: u64, coord: ChunkCoord) -> (i32, i32) {
        let mut world = WorldSim::new_seeded(seed);
        world.ensure_chunk_generated(coord);
        for y in coord.y * CHUNK_SIZE..(coord.y + 1) * CHUNK_SIZE {
            for x in coord.x * CHUNK_SIZE..(coord.x + 1) * CHUNK_SIZE {
                if world
                    .tile_at(x, y)
                    .is_some_and(|tile| tile.collision.walkable)
                {
                    return (x, y);
                }
            }
        }

        panic!("expected a walkable streamed tile");
    }

    fn move_player_to_tile(sim: &mut Simulation, tile: (i32, i32)) {
        let (player_x, player_y) = sim.player().position_tiles();
        sim.move_player_by_tiles(
            tile.0 as f32 + 0.5 - player_x,
            tile.1 as f32 + 0.5 - player_y,
        );
        assert_eq!(sim.player().tile_position(), tile);
    }
}
