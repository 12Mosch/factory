use bevy::asset::RenderAssetUsages;
use bevy::color::Srgba;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};
use std::hash::{Hash, Hasher};

use crate::rendering::colors::{RenderPrototypeIds, resource_color, tile_color};
use crate::rendering::entities::entity_prototype_render_style;
use crate::resources::{MapDisplaySettings, MapTextureBounds, MapTextureCache, SimResource};

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
        || cache.last_resource_revision != sim.sim.world().resource_revision()
        || cache.last_entity_signature != entity_signature
        || cache.last_revealed_signature != revealed_signature
        || cache.last_debug_flags != debug_flags;

    if !needs_update {
        return;
    }

    let map = generate_map_pixels(&sim.sim, &settings);
    let mut image = Image::new_fill(
        Extent3d {
            width: map.bounds.width,
            height: map.bounds.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &UNREVEALED_PIXEL,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(map.data);
    image.sampler = ImageSampler::nearest();

    let handle = match cache.handle.as_ref() {
        Some(handle) => {
            let _ = images.insert(handle.id(), image);
            handle.clone()
        }
        None => images.add(image),
    };

    cache.handle = Some(handle);
    cache.bounds = Some(map.bounds);
    cache.last_player_tile = Some(player_tile);
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
