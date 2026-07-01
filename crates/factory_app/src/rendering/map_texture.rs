use bevy::asset::RenderAssetUsages;
use bevy::color::Srgba;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};
use std::hash::{Hash, Hasher};

use crate::rendering::colors::{RenderPrototypeIds, resource_color, tile_color};
use crate::rendering::entities::entity_prototype_render_style;
use crate::resources::{
    MapChunkPaintState, MapDisplaySettings, MapTextureBounds, MapTextureCache, SimResource,
};

pub const UNREVEALED_PIXEL: [u8; 4] = [6, 7, 8, 255];
pub const PLAYER_PIXEL: [u8; 4] = [245, 245, 240, 255];
pub const GRID_PIXEL: [u8; 4] = [188, 139, 54, 255];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPixels {
    pub bounds: MapTextureBounds,
    pub data: Vec<u8>,
}

pub(crate) fn update_map_texture(
    sim: Res<SimResource>,
    settings: Res<MapDisplaySettings>,
    mut cache: ResMut<MapTextureCache>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut images) = images else {
        return;
    };
    let entity_signature = entity_signature(&sim.sim);
    let revealed_signature = revealed_signature(&sim.sim);
    let debug_flags = (settings.debug_reveal_all, settings.show_chunk_grid);
    let player_tile = sim.sim.player().tile_position();
    let needs_update = cache.handle.is_none()
        || cache.last_player_tile != Some(player_tile)
        || cache.last_chunk_revision != sim.sim.world().chunk_revision()
        || cache.last_resource_revision != sim.sim.world().resource_revision()
        || cache.last_entity_signature != entity_signature
        || cache.last_revealed_signature != revealed_signature
        || cache.last_debug_flags != debug_flags;

    if !needs_update {
        return;
    }

    let full_rebuild = cache.handle.is_none()
        || cache.bounds.is_none()
        || cache.pixels.is_none()
        || cache.last_debug_flags != debug_flags
        || cache.last_entity_signature != entity_signature;

    if full_rebuild {
        let map = generate_map_pixels(&sim.sim, &settings);
        cache.bounds = Some(map.bounds);
        cache.pixels = Some(map.data);
        refresh_painted_chunks(&sim.sim, &settings, &mut cache);
    } else {
        update_map_pixels_incremental(&sim.sim, &settings, &mut cache);
    }

    let bounds = cache.bounds.unwrap_or_default();
    let image_data = cache.pixels.clone().unwrap_or_default();
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
    image.data = Some(image_data);
    image.sampler = ImageSampler::nearest();

    let handle = match cache.handle.as_ref() {
        Some(handle) => {
            let _ = images.insert(handle.id(), image);
            handle.clone()
        }
        None => images.add(image),
    };

    cache.handle = Some(handle);
    cache.last_player_tile = Some(player_tile);
    cache.last_chunk_revision = sim.sim.world().chunk_revision();
    cache.last_resource_revision = sim.sim.world().resource_revision();
    cache.last_entity_signature = entity_signature;
    cache.last_revealed_signature = revealed_signature;
    cache.last_debug_flags = debug_flags;
}

pub fn generate_map_pixels(sim: &Simulation, settings: &MapDisplaySettings) -> MapPixels {
    let bounds = map_texture_bounds(sim).unwrap_or_default();
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
                let terrain = darkened(tile_color(tile.tile_id, ids), 0.58);
                tile.resource
                    .map(|resource| color_to_pixel(resource_color(resource, ids)))
                    .unwrap_or(terrain)
            } else {
                UNREVEALED_PIXEL
            };
            set_world_pixel(&mut data, bounds, world_x, world_y, color);
        }
    }

    for placed in sim.entities().placed_entities() {
        let Some((color, _)) =
            entity_prototype_render_style(sim.catalog(), placed.prototype_id, placed.direction)
        else {
            continue;
        };
        let pixel = color_to_pixel(color);
        for (x, y) in placed.footprint.tiles() {
            let coord = ChunkCoord {
                x: x.div_euclid(CHUNK_SIZE),
                y: y.div_euclid(CHUNK_SIZE),
            };
            if !settings.debug_reveal_all && !sim.is_chunk_revealed(coord) {
                continue;
            }
            set_world_pixel(&mut data, bounds, x, y, pixel);
        }
    }

    let (player_x, player_y) = sim.player().tile_position();
    set_world_pixel(&mut data, bounds, player_x, player_y, PLAYER_PIXEL);

    if settings.show_chunk_grid {
        draw_chunk_grid(&mut data, bounds);
    }

    MapPixels { bounds, data }
}

fn update_map_pixels_incremental(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    cache: &mut MapTextureCache,
) {
    let old_bounds = cache.bounds.unwrap_or_default();
    let new_bounds = map_texture_bounds(sim).unwrap_or_default();
    let bounds_changed = old_bounds != new_bounds;
    if bounds_changed {
        resize_cached_pixels(cache, old_bounds, new_bounds);
    }

    repaint_changed_chunks(sim, settings, cache);
    repaint_dirty_resource_tiles(sim, settings, cache);
    repaint_player_tiles(sim, settings, cache);

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
    cache: &mut MapTextureCache,
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
    cache: &mut MapTextureCache,
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
        paint_chunk(data, bounds, sim, settings, ids, chunk.coord);
        cache.painted_chunks.insert(chunk.coord, state);
    }
}

fn repaint_dirty_resource_tiles(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    cache: &mut MapTextureCache,
) {
    let Some(changes) = sim
        .world()
        .resource_dirty_tiles_since(cache.last_resource_revision)
    else {
        repaint_all_chunks(sim, settings, cache);
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
        paint_tile(data, bounds, sim, settings, ids, change.x, change.y);
    }
}

fn repaint_player_tiles(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    cache: &mut MapTextureCache,
) {
    let player_tile = sim.player().tile_position();
    if cache.last_player_tile == Some(player_tile) {
        return;
    }

    let Some(bounds) = cache.bounds else {
        return;
    };
    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    if let Some((x, y)) = cache.last_player_tile {
        paint_tile(data, bounds, sim, settings, ids, x, y);
    }
    paint_tile(
        data,
        bounds,
        sim,
        settings,
        ids,
        player_tile.0,
        player_tile.1,
    );
}

fn repaint_all_chunks(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    cache: &mut MapTextureCache,
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
        paint_chunk(data, bounds, sim, settings, ids, chunk.coord);
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
    cache: &mut MapTextureCache,
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

pub fn map_texture_bounds(sim: &Simulation) -> Option<MapTextureBounds> {
    let min_chunk_x = sim.world().chunks.keys().map(|coord| coord.x).min()?;
    let max_chunk_x = sim.world().chunks.keys().map(|coord| coord.x).max()?;
    let min_chunk_y = sim.world().chunks.keys().map(|coord| coord.y).min()?;
    let max_chunk_y = sim.world().chunks.keys().map(|coord| coord.y).max()?;

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
        let color = tile_pixel(sim, settings, ids, coord, tile);
        set_world_pixel(data, bounds, world_x, world_y, color);
    }

    if settings.debug_reveal_all || sim.is_chunk_revealed(coord) {
        for placed in sim.entities().placed_entities() {
            let Some((color, _)) =
                entity_prototype_render_style(sim.catalog(), placed.prototype_id, placed.direction)
            else {
                continue;
            };
            let pixel = color_to_pixel(color);
            for (x, y) in placed.footprint.tiles() {
                if x.div_euclid(CHUNK_SIZE) == coord.x && y.div_euclid(CHUNK_SIZE) == coord.y {
                    set_world_pixel(data, bounds, x, y, pixel);
                }
            }
        }
    }

    let player_tile = sim.player().tile_position();
    if player_tile.0.div_euclid(CHUNK_SIZE) == coord.x
        && player_tile.1.div_euclid(CHUNK_SIZE) == coord.y
    {
        set_world_pixel(data, bounds, player_tile.0, player_tile.1, PLAYER_PIXEL);
    }

    if settings.show_chunk_grid {
        draw_chunk_grid_for_chunk(data, bounds, coord);
    }
}

fn paint_tile(
    data: &mut [u8],
    bounds: MapTextureBounds,
    sim: &Simulation,
    settings: &MapDisplaySettings,
    ids: RenderPrototypeIds,
    x: i32,
    y: i32,
) {
    let coord = ChunkCoord {
        x: x.div_euclid(CHUNK_SIZE),
        y: y.div_euclid(CHUNK_SIZE),
    };
    if let Some(tile) = sim.world().tile_at(x, y) {
        let color = tile_pixel(sim, settings, ids, coord, tile);
        set_world_pixel(data, bounds, x, y, color);
    }

    if settings.debug_reveal_all || sim.is_chunk_revealed(coord) {
        for placed in sim.entities().placed_entities() {
            if !placed.footprint.contains_tile(x, y) {
                continue;
            }
            let Some((color, _)) =
                entity_prototype_render_style(sim.catalog(), placed.prototype_id, placed.direction)
            else {
                continue;
            };
            set_world_pixel(data, bounds, x, y, color_to_pixel(color));
        }
    }

    if sim.player().tile_position() == (x, y) {
        set_world_pixel(data, bounds, x, y, PLAYER_PIXEL);
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
    coord: ChunkCoord,
    tile: &factory_sim::TileCell,
) -> [u8; 4] {
    if settings.debug_reveal_all || sim.is_chunk_revealed(coord) {
        let terrain = darkened(tile_color(tile.tile_id, ids), 0.58);
        tile.resource
            .map(|resource| color_to_pixel(resource_color(resource, ids)))
            .unwrap_or(terrain)
    } else {
        UNREVEALED_PIXEL
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

fn draw_chunk_grid(data: &mut [u8], bounds: MapTextureBounds) {
    for y in 0..bounds.height {
        for x in 0..bounds.width {
            let world_x = bounds.min_x + x as i32;
            let world_y = bounds.min_y + y as i32;
            if world_x.rem_euclid(CHUNK_SIZE) == 0 || world_y.rem_euclid(CHUNK_SIZE) == 0 {
                set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
            }
        }
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

fn entity_signature(sim: &Simulation) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for placed in sim.entities().placed_entities() {
        placed.id.raw().hash(&mut hasher);
        placed.prototype_id.hash(&mut hasher);
        placed.x.hash(&mut hasher);
        placed.y.hash(&mut hasher);
        placed.direction.hash(&mut hasher);
        placed.footprint.hash(&mut hasher);
    }
    hasher.finish()
}

fn revealed_signature(sim: &Simulation) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    sim.revealed_chunks().hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::WorldSim;

    #[test]
    fn incremental_update_matches_full_render_after_streaming_chunk() {
        let mut sim = Simulation::new_test_world(123);
        let settings = MapDisplaySettings::default();
        let initial = generate_map_pixels(&sim, &settings);
        let mut cache = MapTextureCache {
            handle: Some(Handle::default()),
            bounds: Some(initial.bounds),
            pixels: Some(initial.data),
            painted_chunks: Default::default(),
            last_player_tile: Some(sim.player().tile_position()),
            last_chunk_revision: sim.world().chunk_revision(),
            last_resource_revision: sim.world().resource_revision(),
            last_entity_signature: entity_signature(&sim),
            last_revealed_signature: revealed_signature(&sim),
            last_debug_flags: (settings.debug_reveal_all, settings.show_chunk_grid),
        };
        refresh_painted_chunks(&sim, &settings, &mut cache);

        let target_chunk = ChunkCoord { x: 0, y: -9 };
        let target = first_walkable_tile_in_chunk(sim.seed(), target_chunk);
        move_player_to_tile(&mut sim, target);
        sim.tick();

        update_map_pixels_incremental(&sim, &settings, &mut cache);

        let full = generate_map_pixels(&sim, &settings);
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
