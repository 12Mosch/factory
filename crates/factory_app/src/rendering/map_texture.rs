use bevy::asset::RenderAssetUsages;
use bevy::color::Srgba;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};

use crate::map::resources::{
    MapChunkPaintState, MapDisplaySettings, MapLayer, MapLayerTextureCache, MapTextureBounds,
    MapTextureCache, MapViewState,
};
use crate::rendering::colors::{RenderPrototypeIds, resource_color, tile_color};
use crate::resources::SimResource;

pub const UNREVEALED_PIXEL: [u8; 4] = [6, 7, 8, 255];
pub const GRID_PIXEL: [u8; 4] = [188, 139, 54, 255];
const RESOURCE_TEXTURE_REFRESH_INTERVAL_TICKS: u64 = 15;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPixels {
    pub bounds: MapTextureBounds,
    pub data: Vec<u8>,
}

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

pub fn generate_map_pixels(sim: &Simulation, settings: &MapDisplaySettings) -> MapPixels {
    generate_map_pixels_for_layer(sim, settings, MapLayer::Surface)
}

pub fn generate_map_pixels_for_layer(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
) -> MapPixels {
    let bounds = map_texture_bounds(sim, settings).unwrap_or_default();
    let len = bounds.width as usize * bounds.height as usize * 4;
    let mut data = vec![0; len];
    let ids = RenderPrototypeIds::from_catalog(sim.catalog());

    for chunk in sim.world().chunks.values() {
        let revealed = settings.debug_reveal_all || sim.is_chunk_revealed(chunk.coord);
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let world_x = chunk.coord.x * CHUNK_SIZE + local_x;
            let world_y = chunk.coord.y * CHUNK_SIZE + local_y;
            let color = if revealed {
                revealed_tile_pixel(layer, ids, tile)
            } else {
                UNREVEALED_PIXEL
            };
            set_world_pixel(&mut data, bounds, world_x, world_y, color);
        }
    }

    if settings.show_chunk_grid {
        draw_chunk_grid(&mut data, bounds);
    }

    MapPixels { bounds, data }
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

    let full_rebuild = cache.bounds.is_none() || cache.pixels.is_none() || debug_changed;
    if full_rebuild {
        let map = generate_map_pixels_for_layer(sim, settings, layer);
        cache.bounds = Some(map.bounds);
        cache.pixels = Some(map.data);
        refresh_painted_chunks(sim, settings, cache);
    } else {
        update_map_pixels_incremental(sim, settings, layer, cache);
    }

    upload_layer_texture(cache, images);

    cache.last_chunk_revision = sim.world().chunk_revision();
    cache.last_resource_revision = sim.world().resource_revision();
    cache.last_revealed_revision = revealed_revision;
    cache.last_debug_flags = debug_flags;
    cache.last_texture_update_tick = tick_count;
}

fn upload_layer_texture(cache: &mut MapLayerTextureCache, images: &mut Assets<Image>) {
    let bounds = cache.bounds.unwrap_or_default();
    let Some(pixels) = cache.pixels.as_ref() else {
        return;
    };

    if let Some(handle) = cache.handle.as_ref()
        && let Some(mut image) = images.get_mut(handle.id())
        && image.width() == bounds.width
        && image.height() == bounds.height
    {
        match image.data.as_mut() {
            Some(data) => data.clone_from(pixels),
            None => image.data = Some(pixels.clone()),
        }
        return;
    }

    let mut image = Image::new_fill(
        Extent3d {
            width: bounds.width,
            height: bounds.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &UNREVEALED_PIXEL,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(pixels.clone());
    image.sampler = ImageSampler::nearest();

    let handle = match cache.handle.as_ref() {
        Some(handle) => {
            let _ = images.insert(handle.id(), image);
            handle.clone()
        }
        None => images.add(image),
    };
    cache.handle = Some(handle);
}

fn should_refresh_texture(last_texture_update_tick: u64, tick_count: u64) -> bool {
    tick_count.saturating_sub(last_texture_update_tick) >= RESOURCE_TEXTURE_REFRESH_INTERVAL_TICKS
}

fn update_map_pixels_incremental(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
    cache: &mut MapLayerTextureCache,
) {
    let old_bounds = cache.bounds.unwrap_or_default();
    let new_bounds = map_texture_bounds(sim, settings).unwrap_or_default();
    let bounds_changed = old_bounds != new_bounds;
    if bounds_changed {
        resize_cached_pixels(cache, old_bounds, new_bounds);
        // The resize only preserves pixels inside both bounds; drop the paint
        // state of clipped chunks so they get repainted at their new position.
        cache.painted_chunks.retain(|coord, _| {
            old_bounds.contains_chunk(*coord) && new_bounds.contains_chunk(*coord)
        });
    }
    repaint_changed_chunks(sim, settings, layer, cache);
    repaint_dirty_resource_tiles(sim, settings, layer, cache);

    if bounds_changed && settings.show_chunk_grid {
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

fn repaint_changed_chunks(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
    cache: &mut MapLayerTextureCache,
) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    cache
        .painted_chunks
        .retain(|coord, _| sim.world().chunks.contains_key(coord));

    for chunk in sim.world().chunks.values() {
        let state = MapChunkPaintState {
            revealed: settings.debug_reveal_all || sim.is_chunk_revealed(chunk.coord),
        };
        if cache.painted_chunks.get(&chunk.coord) == Some(&state) {
            continue;
        }
        paint_chunk(data, bounds, sim, settings, ids, layer, chunk.coord);
        cache.painted_chunks.insert(chunk.coord, state);
    }
}

fn repaint_dirty_resource_tiles(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
    cache: &mut MapLayerTextureCache,
) {
    let Some(changes) = sim
        .world()
        .resource_dirty_tiles_since(cache.last_resource_revision)
    else {
        repaint_all_chunks(sim, settings, layer, cache);
        return;
    };
    let changes = changes.collect::<Vec<_>>();
    if changes.is_empty() {
        return;
    }

    let Some(bounds) = cache.bounds else {
        return;
    };
    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    for change in changes {
        paint_tile(data, bounds, sim, settings, ids, layer, change.x, change.y);
    }
}

fn repaint_all_chunks(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    layer: MapLayer,
    cache: &mut MapLayerTextureCache,
) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    cache.painted_chunks.clear();
    for chunk in sim.world().chunks.values() {
        paint_chunk(data, bounds, sim, settings, ids, layer, chunk.coord);
        cache.painted_chunks.insert(
            chunk.coord,
            MapChunkPaintState {
                revealed: settings.debug_reveal_all || sim.is_chunk_revealed(chunk.coord),
            },
        );
    }
}

fn refresh_painted_chunks(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    cache: &mut MapLayerTextureCache,
) {
    cache.painted_chunks = sim
        .world()
        .chunks
        .keys()
        .copied()
        .map(|coord| {
            (
                coord,
                MapChunkPaintState {
                    revealed: settings.debug_reveal_all || sim.is_chunk_revealed(coord),
                },
            )
        })
        .collect();
}

pub fn map_texture_bounds(
    sim: &Simulation,
    settings: &MapDisplaySettings,
) -> Option<MapTextureBounds> {
    if settings.debug_reveal_all {
        chunk_texture_bounds(sim.world().chunks.keys().copied())
    } else {
        chunk_texture_bounds(
            sim.revealed_chunks()
                .iter()
                .copied()
                .filter(|coord| sim.world().chunks.contains_key(coord)),
        )
    }
}

fn chunk_texture_bounds(
    chunk_coords: impl IntoIterator<Item = ChunkCoord>,
) -> Option<MapTextureBounds> {
    let mut chunk_coords = chunk_coords.into_iter();
    let first = chunk_coords.next()?;
    let mut min_chunk_x = first.x;
    let mut max_chunk_x = first.x;
    let mut min_chunk_y = first.y;
    let mut max_chunk_y = first.y;

    for coord in chunk_coords {
        min_chunk_x = min_chunk_x.min(coord.x);
        max_chunk_x = max_chunk_x.max(coord.x);
        min_chunk_y = min_chunk_y.min(coord.y);
        max_chunk_y = max_chunk_y.max(coord.y);
    }

    Some(MapTextureBounds {
        min_x: min_chunk_x * CHUNK_SIZE,
        min_y: min_chunk_y * CHUNK_SIZE,
        width: ((max_chunk_x - min_chunk_x + 1) * CHUNK_SIZE) as u32,
        height: ((max_chunk_y - min_chunk_y + 1) * CHUNK_SIZE) as u32,
    })
}

fn paint_chunk(
    data: &mut [u8],
    bounds: MapTextureBounds,
    sim: &Simulation,
    settings: &MapDisplaySettings,
    ids: RenderPrototypeIds,
    layer: MapLayer,
    coord: ChunkCoord,
) {
    let Some(chunk) = sim.world().chunks.get(&coord) else {
        return;
    };

    for (index, tile) in chunk.tiles.iter().enumerate() {
        let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
        let local_y = (index as i32).div_euclid(CHUNK_SIZE);
        let world_x = chunk.coord.x * CHUNK_SIZE + local_x;
        let world_y = chunk.coord.y * CHUNK_SIZE + local_y;
        let color = tile_pixel(sim, settings, ids, layer, coord, tile);
        set_world_pixel(data, bounds, world_x, world_y, color);
    }

    if settings.show_chunk_grid {
        draw_chunk_grid_for_chunk(data, bounds, coord);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_tile(
    data: &mut [u8],
    bounds: MapTextureBounds,
    sim: &Simulation,
    settings: &MapDisplaySettings,
    ids: RenderPrototypeIds,
    layer: MapLayer,
    x: i32,
    y: i32,
) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    if let Some(tile) = sim.world().tile_at(x, y) {
        let color = tile_pixel(sim, settings, ids, layer, coord, tile);
        set_world_pixel(data, bounds, x, y, color);
    }

    if settings.show_chunk_grid && (x.rem_euclid(CHUNK_SIZE) == 0 || y.rem_euclid(CHUNK_SIZE) == 0)
    {
        set_world_pixel(data, bounds, x, y, GRID_PIXEL);
    }
}

fn tile_pixel(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    ids: RenderPrototypeIds,
    layer: MapLayer,
    coord: ChunkCoord,
    tile: &factory_sim::TileCell,
) -> [u8; 4] {
    if settings.debug_reveal_all || sim.is_chunk_revealed(coord) {
        revealed_tile_pixel(layer, ids, tile)
    } else {
        UNREVEALED_PIXEL
    }
}

fn revealed_tile_pixel(
    layer: MapLayer,
    ids: RenderPrototypeIds,
    tile: &factory_sim::TileCell,
) -> [u8; 4] {
    match layer {
        MapLayer::Surface => {
            let terrain = darkened(tile_color(tile.tile_id, ids), 0.58);
            tile.resource
                .map(|resource| color_to_pixel(resource_color(resource, ids)))
                .unwrap_or(terrain)
        }
        MapLayer::Resources => tile
            .resource
            .map(|resource| color_to_pixel(resource_color(resource, ids)))
            .unwrap_or_else(|| darkened(tile_color(tile.tile_id, ids), 0.24)),
        MapLayer::Entities => darkened(tile_color(tile.tile_id, ids), 0.30),
    }
}

fn set_world_pixel(data: &mut [u8], bounds: MapTextureBounds, x: i32, y: i32, pixel: [u8; 4]) {
    let local_x = x - bounds.min_x;
    let local_y = y - bounds.min_y;
    if local_x < 0 || local_y < 0 {
        return;
    }
    let local_x = local_x as u32;
    let local_y = local_y as u32;
    if local_x >= bounds.width || local_y >= bounds.height {
        return;
    }
    let flipped_y = bounds.height - 1 - local_y;
    let offset = ((flipped_y * bounds.width + local_x) * 4) as usize;
    data[offset..offset + 4].copy_from_slice(&pixel);
}

fn pixel_offset(bounds: MapTextureBounds, x: i32, y: i32) -> usize {
    let local_x = (x - bounds.min_x) as u32;
    let local_y = (y - bounds.min_y) as u32;
    let flipped_y = bounds.height - 1 - local_y;
    ((flipped_y * bounds.width + local_x) * 4) as usize
}

fn next_chunk_boundary_at_or_after(value: i32) -> i32 {
    value.div_euclid(CHUNK_SIZE) * CHUNK_SIZE
        + if value.rem_euclid(CHUNK_SIZE) == 0 {
            0
        } else {
            CHUNK_SIZE
        }
}

fn draw_chunk_grid(data: &mut [u8], bounds: MapTextureBounds) {
    let max_x = bounds.min_x + bounds.width as i32 - 1;
    let max_y = bounds.min_y + bounds.height as i32 - 1;

    let mut world_x = next_chunk_boundary_at_or_after(bounds.min_x);
    while world_x <= max_x {
        for world_y in bounds.min_y..=max_y {
            set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
        }
        world_x += CHUNK_SIZE;
    }

    let mut world_y = next_chunk_boundary_at_or_after(bounds.min_y);
    while world_y <= max_y {
        for world_x in bounds.min_x..=max_x {
            set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
        }
        world_y += CHUNK_SIZE;
    }
}

fn draw_chunk_grid_for_chunk(data: &mut [u8], bounds: MapTextureBounds, coord: ChunkCoord) {
    let min_x = coord.x * CHUNK_SIZE;
    let min_y = coord.y * CHUNK_SIZE;
    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let world_x = min_x + local_x;
            let world_y = min_y + local_y;
            if world_x.rem_euclid(CHUNK_SIZE) == 0 || world_y.rem_euclid(CHUNK_SIZE) == 0 {
                set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
            }
        }
    }
}

fn darkened(color: Color, factor: f32) -> [u8; 4] {
    let srgba = color.to_srgba();
    Srgba::new(
        srgba.red * factor,
        srgba.green * factor,
        srgba.blue * factor,
        srgba.alpha,
    )
    .to_u8_array()
}

fn color_to_pixel(color: Color) -> [u8; 4] {
    color.to_srgba().to_u8_array()
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::WorldSim;

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
            refresh_painted_chunks(&sim, &settings, &mut cache);
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
            update_map_pixels_incremental(&sim, &settings, *layer, cache);

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
    fn draw_chunk_grid_paints_exactly_the_chunk_boundary_pixels() {
        let bounds = MapTextureBounds {
            min_x: -CHUNK_SIZE * 2,
            min_y: -CHUNK_SIZE,
            width: (CHUNK_SIZE * 3) as u32,
            height: (CHUNK_SIZE * 2) as u32,
        };
        let mut data = vec![0; bounds.width as usize * bounds.height as usize * 4];

        draw_chunk_grid(&mut data, bounds);

        for world_y in bounds.min_y..bounds.min_y + bounds.height as i32 {
            for world_x in bounds.min_x..bounds.min_x + bounds.width as i32 {
                let offset = pixel_offset(bounds, world_x, world_y);
                let expected =
                    if world_x.rem_euclid(CHUNK_SIZE) == 0 || world_y.rem_euclid(CHUNK_SIZE) == 0 {
                        GRID_PIXEL
                    } else {
                        [0; 4]
                    };
                assert_eq!(
                    data[offset..offset + 4],
                    expected,
                    "pixel at ({world_x}, {world_y})"
                );
            }
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
        refresh_painted_chunks(&sim, &settings, &mut cache);

        // Frontier chunk grows the texture bounds by one chunk column.
        sim.ensure_chunk_generated(ChunkCoord { x: 16, y: 0 });

        let started = std::time::Instant::now();
        update_map_pixels_incremental(&sim, &settings, MapLayer::Surface, &mut cache);
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
