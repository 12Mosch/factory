use bevy::prelude::*;
use factory_sim::CHUNK_SIZE;

use crate::constants::TILE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, tile_color};
use crate::rendering::transforms::tile_translation;
use crate::resources::SimResource;

pub(crate) fn spawn_world_tiles(mut commands: Commands, sim: Res<SimResource>) {
    let ids = RenderPrototypeIds::from_catalog(sim.sim.catalog());

    for chunk in sim.sim.world().chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let world_x = chunk.coord.x * CHUNK_SIZE + local_x;
            let world_y = chunk.coord.y * CHUNK_SIZE + local_y;
            let translation = tile_translation(world_x, world_y, 0.0);

            commands.spawn((
                Sprite::from_color(tile_color(tile.tile_id, ids), Vec2::splat(TILE_SIZE)),
                Transform::from_translation(translation),
            ));
        }
    }
}
