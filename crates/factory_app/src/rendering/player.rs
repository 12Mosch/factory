use bevy::prelude::*;
use std::time::Instant;

use crate::constants::PLAYER_SPRITE_SIZE;
use crate::rendering::transforms::player_translation;
use crate::resources::{RenderSyncStats, SimResource};

#[derive(Component)]
pub(crate) struct PlayerSprite;

pub(crate) fn spawn_player(mut commands: Commands, sim: Res<SimResource>) {
    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.92, 0.84, 0.42),
            Vec2::splat(PLAYER_SPRITE_SIZE),
        ),
        Transform::from_translation(player_translation(sim.sim.player(), 4.0)),
        PlayerSprite,
    ));
}

pub(crate) fn sync_player_sprite(
    sim: Res<SimResource>,
    mut players: Query<&mut Transform, With<PlayerSprite>>,
) {
    for mut transform in &mut players {
        transform.translation = player_translation(sim.sim.player(), transform.translation.z);
    }
}

pub(crate) fn measured_sync_player_sprite(
    sim: Res<SimResource>,
    players: Query<&mut Transform, With<PlayerSprite>>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_player_sprite(sim, players);
    stats.record_player(started.elapsed());
}
