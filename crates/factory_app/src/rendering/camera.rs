use bevy::prelude::*;

use crate::constants::INITIAL_CAMERA_SCALE;
use crate::rendering::player::PlayerSprite;
use crate::rendering::transforms::player_translation;
use crate::resources::SimResource;

pub(crate) fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: INITIAL_CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
}

pub(crate) fn follow_player_camera(
    sim: Res<SimResource>,
    mut cameras: Query<&mut Transform, (With<Camera2d>, Without<PlayerSprite>)>,
) {
    let player = player_translation(sim.sim.player(), 0.0);

    for mut transform in &mut cameras {
        transform.translation.x = player.x;
        transform.translation.y = player.y;
    }
}
