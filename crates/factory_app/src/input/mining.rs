use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{ManualMiningTarget, SimCommand};

use crate::input::panels::world_input_blocked;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::{AppInputState, TechnologyWindowState};
use crate::simulation::SimCommandRequest;

pub(crate) fn update_manual_mining_from_input(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let blocked = world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open);

    let target = if blocked {
        None
    } else {
        mouse
            .filter(|mouse| mouse.pressed(MouseButton::Right))
            .and_then(|_| cursor_tile_from_window(&windows, &cameras))
            .map(|(x, y)| ManualMiningTarget { x, y })
    };

    commands.write(SimCommandRequest(SimCommand::SetManualMiningTarget(target)));
}
