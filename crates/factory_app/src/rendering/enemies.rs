use bevy::prelude::*;
use factory_sim::EnemyId;
use std::collections::{HashMap, HashSet};

use crate::constants::TILE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::enemy_unit_color;
use crate::resources::SimResource;

const ENEMY_SPRITE_SIZE: f32 = TILE_SIZE * 0.55;
const ENEMY_SPRITE_Z: f32 = 4.5;

#[derive(Component)]
#[allow(dead_code)] // Retained as render-world identity for diagnostics/tests.
pub(crate) struct EnemySprite {
    enemy_id: EnemyId,
}

/// Persistent identity and reusable membership scratch for enemy visuals.
#[derive(Default)]
pub(crate) struct EnemyRenderState {
    initialized: bool,
    synced_tick: u64,
    visible_revision: u64,
    sim_replacement_revision: u64,
    entities: HashMap<EnemyId, Entity>,
    visible_ids: HashSet<EnemyId>,
    removed_ids: Vec<EnemyId>,
}

/// Samples enemy positions once per completed simulation tick. Unchanged
/// rendered frames do no enemy lookup, transform write, allocation, or scan.
pub(crate) fn sync_enemy_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    mut state: Local<EnemyRenderState>,
    mut sprites: Query<&mut Transform, With<EnemySprite>>,
) {
    let state = &mut *state;
    let replacement_revision = sim.replacement_revision();
    let sim = sim.read();
    let tick = sim.tick_count();
    let replaced = !state.initialized || state.sim_replacement_revision != replacement_revision;
    let tick_changed = !state.initialized || state.synced_tick != tick;
    let view_changed = !state.initialized || state.visible_revision != visible.revision;
    if !replaced && !tick_changed && !view_changed {
        return;
    }

    if replaced {
        for (_, render_entity) in state.entities.drain() {
            commands.entity(render_entity).despawn();
        }
    }

    state.visible_ids.clear();
    for &chunk in &visible.chunks {
        state.visible_ids.extend(
            sim.enemy_ids_in_chunk(chunk)
                .iter()
                .copied()
                .filter(|enemy_id| sim.enemies().get(*enemy_id).is_some()),
        );
    }

    state.removed_ids.clear();
    for &enemy_id in state.entities.keys() {
        if !state.visible_ids.contains(&enemy_id) {
            state.removed_ids.push(enemy_id);
        }
    }
    while let Some(enemy_id) = state.removed_ids.pop() {
        if let Some(render_entity) = state.entities.remove(&enemy_id) {
            commands.entity(render_entity).despawn();
        }
    }

    // Clone into retained scratch capacity so the registry can be mutated
    // while walking the current IDs without allocating a fresh seen set.
    state.removed_ids.extend(state.visible_ids.iter().copied());
    while let Some(enemy_id) = state.removed_ids.pop() {
        let Some(enemy) = sim.enemies().get(enemy_id) else {
            continue;
        };
        let (x, y) = enemy.position_tiles();
        let translation = Vec3::new(x * TILE_SIZE, y * TILE_SIZE, ENEMY_SPRITE_Z);
        if let Some(&render_entity) = state.entities.get(&enemy_id) {
            if tick_changed
                && let Ok(mut transform) = sprites.get_mut(render_entity)
                && transform.translation != translation
            {
                transform.translation = translation;
            }
            continue;
        }

        let render_entity = commands
            .spawn((
                Sprite::from_color(enemy_unit_color(), Vec2::splat(ENEMY_SPRITE_SIZE)),
                Transform::from_translation(translation),
                EnemySprite { enemy_id },
            ))
            .id();
        state.entities.insert(enemy_id, render_entity);
    }

    state.initialized = true;
    state.synced_tick = tick;
    state.visible_revision = visible.revision;
    state.sim_replacement_revision = replacement_revision;
}
