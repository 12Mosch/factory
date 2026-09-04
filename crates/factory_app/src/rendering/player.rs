use bevy::prelude::*;
use std::time::Instant;

use crate::constants::PLAYER_SPRITE_SIZE;
use crate::rendering::resources::PlayerRenderSyncTime;
use crate::rendering::transforms::player_translation;
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct PlayerSprite;

/// Spawns the active world's player sprite unless the retained sprite already exists.
pub(crate) fn spawn_player(
    mut commands: Commands,
    sim: Res<SimResource>,
    existing: Query<(), With<PlayerSprite>>,
) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.92, 0.84, 0.42),
            Vec2::splat(PLAYER_SPRITE_SIZE),
        ),
        Transform::from_translation(player_translation(sim.read().player(), 4.0)),
        PlayerSprite,
    ));
}

pub(crate) fn sync_player_sprite(
    sim: Res<SimResource>,
    mut players: Query<&mut Transform, With<PlayerSprite>>,
) {
    for mut transform in &mut players {
        transform.translation = player_translation(sim.read().player(), transform.translation.z);
    }
}

pub(crate) fn measured_sync_player_sprite(
    sim: Res<SimResource>,
    players: Query<&mut Transform, With<PlayerSprite>>,
    mut timing: ResMut<PlayerRenderSyncTime>,
) {
    let started = Instant::now();
    sync_player_sprite(sim, players);
    timing.0 = started.elapsed();
}
