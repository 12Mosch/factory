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
use super::upload::{MapTextureUploadQueue, upload_layer_texture};

const RESOURCE_TEXTURE_REFRESH_INTERVAL_TICKS: u64 = 15;

pub(crate) fn update_map_texture(
    sim: Res<SimResource>,
    settings: Res<MapDisplaySettings>,
    state: Res<MapViewState>,
    mut cache: ResMut<MapTextureCache>,
    mut uploads: ResMut<MapTextureUploadQueue>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut images) = images else {
        return;
    };

    // The surface layer also backs the minimap, so it stays fresh even while
    // the fullscreen map is closed. Other layers only update while displayed.
    let surface_cache = cache.layer_mut(MapLayer::Surface);
    update_layer_map_texture(
        &sim.read(),
        &settings,
        MapLayer::Surface,
        surface_cache,
        &mut images,
        &mut uploads,
    );

    if state.open && state.selected_layer != MapLayer::Surface {
        let layer_cache = cache.layer_mut(state.selected_layer);
        update_layer_map_texture(
            &sim.read(),
            &settings,
            state.selected_layer,
            layer_cache,
            &mut images,
            &mut uploads,
        );
    }
}

fn update_layer_map_texture(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
    cache: &mut MapLayerTextureCache,
    images: &mut Assets<Image>,
    uploads: &mut MapTextureUploadQueue,
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
        cache.dirty_regions.mark_full();
        refresh_painted_chunks(&rasterizer, cache);
    } else {
        update_map_pixels_incremental(&rasterizer, cache);
    }

    upload_layer_texture(cache, images, uploads);

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
        cache.dirty_regions.mark_full();
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
        cache.dirty_regions.mark_world_chunk(bounds, chunk.coord);
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
        cache
            .dirty_regions
            .mark_world_tile(bounds, change.x, change.y);
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
    cache.dirty_regions.mark_full();
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
    use crate::map::resources::MapChunkPaintState;
    use crate::rendering::map_texture::{UNREVEALED_PIXEL, generate_map_pixels_for_layer};
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    use factory_sim::{CHUNK_SIZE, ChunkCoord, ManualMiningTarget, WorldSim};
    use std::hint::black_box;

    fn image_asset(width: u32, height: u32, data: Option<Vec<u8>>) -> Image {
        let mut image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &UNREVEALED_PIXEL,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.data = data;
        image.sampler = ImageSampler::nearest();
        image
    }

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
                dirty_regions: Default::default(),
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
    fn changed_chunk_queues_chunk_rect_upload() {
        let sim = Simulation::new_test_world(123);
        let settings = MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: false,
        };
        let layer = MapLayer::Surface;
        let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
        let mut images = Assets::<Image>::default();
        let handle = images.add(image_asset(
            initial.bounds.width,
            initial.bounds.height,
            None,
        ));
        let mut cache = MapLayerTextureCache {
            handle: Some(handle.clone()),
            bounds: Some(initial.bounds),
            pixels: Some(initial.data),
            dirty_regions: Default::default(),
            painted_chunks: Default::default(),
            last_chunk_revision: sim.world().chunk_revision(),
            last_resource_revision: sim.world().resource_revision(),
            last_revealed_revision: sim.revealed_revision(),
            last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
            last_texture_update_tick: sim.tick_count(),
        };
        let rasterizer = MapRasterizer::new(&sim, &settings, layer);
        refresh_painted_chunks(&rasterizer, &mut cache);
        let target = *sim
            .world()
            .chunks
            .keys()
            .next()
            .expect("test world should have chunks");
        cache
            .painted_chunks
            .insert(target, MapChunkPaintState { revealed: false });

        update_map_pixels_incremental(&rasterizer, &mut cache);
        let expected_rect = {
            let mut regions = crate::map::resources::MapTextureDirtyRegions::default();
            regions.mark_world_chunk(initial.bounds, target);
            regions.rects()[0]
        };
        assert_eq!(cache.dirty_regions.rects(), &[expected_rect]);

        let mut uploads = MapTextureUploadQueue::default();
        upload_layer_texture(&mut cache, &mut images, &mut uploads);

        assert_eq!(uploads.commands.len(), 1);
        assert_eq!(uploads.commands[0].rect, expected_rect);
        assert_eq!(
            uploads.commands[0].data.len(),
            expected_rect.width as usize * expected_rect.height as usize * 4
        );
        assert!(
            images
                .get(handle.id())
                .is_some_and(|image| image.data.is_none())
        );
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
            dirty_regions: Default::default(),
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

    #[test]
    #[ignore]
    fn bench_resource_tile_partial_upload_vs_full_buffer_upload() {
        const ITERATIONS: usize = 64;

        let mut sim = Simulation::new_test_world(123);
        for y in -40..=40 {
            for x in -40..=40 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
        let settings = MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: false,
        };
        let layer = MapLayer::Resources;
        let initial = generate_map_pixels_for_layer(&sim, &settings, layer);
        let mut images = Assets::<Image>::default();
        let handle = images.add(image_asset(
            initial.bounds.width,
            initial.bounds.height,
            Some(initial.data.clone()),
        ));
        let mut cache = MapLayerTextureCache {
            handle: Some(handle),
            bounds: Some(initial.bounds),
            pixels: Some(initial.data),
            dirty_regions: Default::default(),
            painted_chunks: Default::default(),
            last_chunk_revision: sim.world().chunk_revision(),
            last_resource_revision: sim.world().resource_revision(),
            last_revealed_revision: sim.revealed_revision(),
            last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
            last_texture_update_tick: sim.tick_count(),
        };
        let rasterizer = MapRasterizer::new(&sim, &settings, layer);
        refresh_painted_chunks(&rasterizer, &mut cache);

        let resource_tile = resource_tile_with_minimum_amount(&sim, ITERATIONS as u32)
            .expect("large generated map should contain enough resource amount");
        move_player_to_tile(&mut sim, resource_tile);

        let full_buffer_bytes = cache.pixels.as_ref().expect("pixels").len();
        let full_started = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            let copied = cache.pixels.as_ref().expect("pixels").clone();
            black_box(copied);
        }
        let full_elapsed = full_started.elapsed();

        let mut dirty_upload_bytes = 0usize;
        let dirty_started = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            mine_one_resource(&mut sim, resource_tile);
            let rasterizer = MapRasterizer::new(&sim, &settings, layer);
            update_map_pixels_incremental(&rasterizer, &mut cache);

            let mut uploads = MapTextureUploadQueue::default();
            upload_layer_texture(&mut cache, &mut images, &mut uploads);
            dirty_upload_bytes += uploads
                .commands
                .iter()
                .map(|command| command.data.len())
                .sum::<usize>();
            cache.last_resource_revision = sim.world().resource_revision();
        }
        let dirty_elapsed = dirty_started.elapsed();
        let dirty_upload_bytes_per_iteration = dirty_upload_bytes / ITERATIONS;

        println!(
            "texture size: {}x{}",
            initial.bounds.width, initial.bounds.height
        );
        println!("full buffer bytes: {full_buffer_bytes}");
        println!("dirty upload bytes: {dirty_upload_bytes_per_iteration}");
        println!("old simulated full upload packaging time: {full_elapsed:?}");
        println!("new dirty upload packaging time: {dirty_elapsed:?}");
        println!(
            "byte reduction ratio: {:.2}x",
            full_buffer_bytes as f64 / dirty_upload_bytes_per_iteration as f64
        );
        println!(
            "timing ratio: {:.2}x",
            full_elapsed.as_secs_f64() / dirty_elapsed.as_secs_f64()
        );

        assert_eq!(dirty_upload_bytes_per_iteration, 4);
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

    fn resource_tile_with_minimum_amount(
        sim: &Simulation,
        minimum_amount: u32,
    ) -> Option<(i32, i32)> {
        sim.world()
            .chunks
            .values()
            .flat_map(|chunk| {
                chunk
                    .tiles
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, tile)| {
                        let resource = tile.resource?;
                        if resource.amount < minimum_amount {
                            return None;
                        }
                        let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                        let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                        Some((
                            chunk.coord.x * CHUNK_SIZE + local_x,
                            chunk.coord.y * CHUNK_SIZE + local_y,
                        ))
                    })
            })
            .next()
    }

    fn mine_one_resource(sim: &mut Simulation, tile: (i32, i32)) {
        let before = sim.world().resource_revision();
        let target = Some(ManualMiningTarget {
            x: tile.0,
            y: tile.1,
        });
        for _ in 0..1_000 {
            sim.update_manual_mining(target);
            if sim.world().resource_revision() != before {
                return;
            }
        }

        panic!("manual mining did not update resource revision");
    }
}
