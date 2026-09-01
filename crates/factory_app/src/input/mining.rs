use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{ManualMiningTarget, SimCommand};

use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::input::train_manual::stock_at_tile;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

pub(crate) fn update_manual_mining_from_input(
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    sim: Res<SimResource>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let blocked = world_input_blocked(input_state.as_deref())
        || technology_window.as_deref().is_some_and(|state| state.open);

    let cursor = if blocked {
        None
    } else {
        actions
            .pressed(InputAction::Secondary)
            .then(|| cursor_tile_from_window(&windows, &cameras))
            .flatten()
    };

    // Rolling stock comes off the rails in one action rather than by chipping
    // away at it: it is not a placed entity, so there is no mining progress to
    // accumulate against and nothing in the occupancy grid for the manual
    // mining target to find. The press edge rather than the held button is what
    // keeps a held right-click from trying to mine the same wagon every frame.
    let stock_under_cursor = cursor.and_then(|(x, y)| stock_at_tile(&sim.read(), x, y));
    if let Some(stock_id) = stock_under_cursor
        && actions.just_pressed(InputAction::Secondary)
    {
        commands.write(SimCommandRequest(SimCommand::MineRollingStock { stock_id }));
    }

    // A tile with a wagon on it is the wagon's, so the terrain under it is not
    // also mined out from under the train while the button is held.
    let target = cursor
        .filter(|_| stock_under_cursor.is_none())
        .map(|(x, y)| ManualMiningTarget { x, y });
    commands.write(SimCommandRequest(SimCommand::SetManualMiningTarget(target)));
}
