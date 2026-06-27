use bevy::prelude::*;
use bevy::time::Fixed;

use crate::resources::SimResource;

pub(crate) fn move_player_from_input(
    time: Res<Time<Fixed>>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut sim: ResMut<SimResource>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    let direction = movement_direction_from_keyboard(&keyboard);
    if direction != Vec2::ZERO {
        sim.sim
            .move_player(direction.x, direction.y, time.delta_secs());
    }
}

fn movement_direction_from_keyboard(keyboard: &ButtonInput<KeyCode>) -> Vec2 {
    let mut direction = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    direction
}
