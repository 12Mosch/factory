use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::ManualMiningTarget;

use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;

pub(crate) fn update_manual_mining_from_input(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut sim: ResMut<SimResource>,
) {
    let target = mouse
        .filter(|mouse| mouse.pressed(MouseButton::Right))
        .and_then(|_| cursor_tile_from_window(&windows, &cameras))
        .map(|(x, y)| ManualMiningTarget { x, y });

    sim.sim.update_manual_mining(target);
}
