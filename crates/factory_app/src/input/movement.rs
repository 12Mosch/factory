use bevy::prelude::*;
use bevy::time::Fixed;
use factory_sim::SimCommand;

use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

pub(crate) fn move_player_from_input(
    time: Res<Time<Fixed>>,
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        return;
    }

    let direction = movement_direction(&actions);
    if direction != Vec2::ZERO {
        commands.write(SimCommandRequest(SimCommand::MovePlayer {
            direction_x: direction.x,
            direction_y: direction.y,
            delta_seconds: time.delta_secs(),
        }));
    }
}

fn movement_direction(actions: &ActionInput) -> Vec2 {
    let mut direction = Vec2::ZERO;
    if actions.pressed(InputAction::MoveUp) {
        direction.y += 1.0;
    }
    if actions.pressed(InputAction::MoveDown) {
        direction.y -= 1.0;
    }
    if actions.pressed(InputAction::MoveLeft) {
        direction.x -= 1.0;
    }
    if actions.pressed(InputAction::MoveRight) {
        direction.x += 1.0;
    }

    direction
}
