use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::ManualMiningTarget;

use crate::input::panels::world_input_blocked;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::{AppInputState, SimResource, TechnologyWindowState};

pub(crate) fn update_manual_mining_from_input(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut sim: ResMut<SimResource>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open)
    {
        sim.sim.update_manual_mining(None);
        return;
    }

    let target = mouse
        .filter(|mouse| mouse.pressed(MouseButton::Right))
        .and_then(|_| cursor_tile_from_window(&windows, &cameras))
        .map(|(x, y)| ManualMiningTarget { x, y });

    sim.sim.update_manual_mining(target);
}
