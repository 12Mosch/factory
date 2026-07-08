use bevy::prelude::*;
use bevy::time::Fixed;
use factory_sim::SimCommand;

use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

pub(crate) fn move_player_from_input(
    time: Res<Time<Fixed>>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        return;
    }

    let Some(keyboard) = keyboard else {
        return;
    };

    let direction = movement_direction_from_keyboard(&keyboard);
    if direction != Vec2::ZERO {
        commands.write(SimCommandRequest(SimCommand::MovePlayer {
            direction_x: direction.x,
            direction_y: direction.y,
            delta_seconds: time.delta_secs(),
        }));
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
