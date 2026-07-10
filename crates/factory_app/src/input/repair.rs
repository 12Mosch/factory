use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::SimCommand;

use crate::build::resources::BuildPlacementState;
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

#[derive(SystemParam)]
pub(crate) struct RepairInputState<'w> {
    keyboard: Option<Res<'w, ButtonInput<KeyCode>>>,
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    build_state: Res<'w, BuildPlacementState>,
    sim: Res<'w, SimResource>,
}

/// Holding R over a damaged entity repairs it, consuming repair packs. The
/// command repeats every frame while held; the simulation rate-limits the
/// restored health per application.
pub(crate) fn update_repair_from_input(
    state: RepairInputState,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(keyboard) = state.keyboard.as_deref() else {
        return;
    };
    // R rotates while a building is selected; repair only applies otherwise.
    if !keyboard.pressed(KeyCode::KeyR)
        || state.build_state.selected.is_some()
        || world_input_blocked(state.input_state.as_deref())
        || state
            .technology_window
            .as_deref()
            .is_some_and(|state| state.open)
    {
        return;
    }

    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };
    let sim = state.sim.read();
    let Some(entity_id) = sim.entities().occupancy().entity_at(x, y) else {
        return;
    };
    // Only issue the command when it can do something: the entity is
    // damaged and repairable. Errors like missing packs still surface
    // through command feedback.
    let Some((current, max)) = sim.entity_health(entity_id) else {
        return;
    };
    if current >= max {
        return;
    }

    commands.write(SimCommandRequest(SimCommand::RepairEntity { entity_id }));
}
