use bevy::prelude::*;

use crate::constants::PLAYER_SPRITE_SIZE;
use crate::rendering::transforms::player_translation;
use crate::resources::SimResource;

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
