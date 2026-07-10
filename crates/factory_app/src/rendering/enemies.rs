use bevy::prelude::*;
use factory_sim::{ChunkCoord, EnemyId};
use std::collections::HashMap;

use crate::constants::TILE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::enemy_unit_color;
use crate::resources::SimResource;

const ENEMY_SPRITE_SIZE: f32 = TILE_SIZE * 0.55;
const ENEMY_SPRITE_Z: f32 = 4.5;

#[derive(Component)]
pub(crate) struct EnemySprite {
    enemy_id: EnemyId,
}

/// Mirrors simulation enemy units into render sprites: moves surviving
/// units, despawns dead ones, and spawns newcomers.
pub(crate) fn sync_enemy_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut sprites: Query<(Entity, &EnemySprite, &mut Transform)>,
) {
    let sim = sim.read();
    let mut seen: HashMap<EnemyId, ()> = HashMap::new();

    for (entity, marker, mut transform) in &mut sprites {
        if let Some(enemy) = sim.enemies().get(marker.enemy_id)
            && enemy_is_visible(enemy, &visible)
        {
            let (x, y) = enemy.position_tiles();
            transform.translation = Vec3::new(x * TILE_SIZE, y * TILE_SIZE, ENEMY_SPRITE_Z);
            seen.insert(marker.enemy_id, ());
        } else {
            commands.entity(entity).despawn();
        }
    }

    for enemy in sim.enemies().iter() {
        if seen.contains_key(&enemy.id) || !enemy_is_visible(enemy, &visible) {
            continue;
        }
        let (x, y) = enemy.position_tiles();
        commands.spawn((
            Sprite::from_color(enemy_unit_color(), Vec2::splat(ENEMY_SPRITE_SIZE)),
            Transform::from_translation(Vec3::new(x * TILE_SIZE, y * TILE_SIZE, ENEMY_SPRITE_Z)),
            EnemySprite { enemy_id: enemy.id },
        ));
    }
}

fn enemy_is_visible(enemy: &factory_sim::Enemy, visible: &VisibleChunks) -> bool {
    let (x, y) = enemy.tile();
    ChunkCoord::from_tile(x, y).is_some_and(|coord| visible.chunks.contains(&coord))
}
