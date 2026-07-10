use bevy::prelude::*;
use factory_sim::{EntityFootprint, PlayerState, WorldTileCoord};

use crate::constants::{MANUAL_MINING_BAR_Y_OFFSET, TILE_SIZE};

pub(crate) fn tile_translation(x: WorldTileCoord, y: WorldTileCoord, z: f32) -> Vec3 {
    Vec3::new(
        x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        y as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        z,
    )
}

pub(crate) fn entity_translation(footprint: &EntityFootprint, z: f32) -> Vec3 {
    Vec3::new(
        footprint.x as f32 * TILE_SIZE + footprint.width as f32 * TILE_SIZE * 0.5,
        footprint.y as f32 * TILE_SIZE + footprint.height as f32 * TILE_SIZE * 0.5,
        z,
    )
}

pub(crate) fn manual_mining_bar_translation(x: WorldTileCoord, y: WorldTileCoord, z: f32) -> Vec3 {
    let mut translation = tile_translation(x, y, z);
    translation.y += MANUAL_MINING_BAR_Y_OFFSET;
    translation
}

pub(crate) fn player_translation(player: PlayerState, z: f32) -> Vec3 {
    let (x, y) = player.position_tiles();
    Vec3::new(x * TILE_SIZE, y * TILE_SIZE, z)
}
