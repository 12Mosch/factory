use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{EntityId, Simulation};

use crate::build::resources::{BuildPlacementState, PlannerState, PlannerTool};
use crate::input::panels::{escape_consumed, world_input_blocked};
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::interaction::machine_kind::open_machine_kind;
use crate::resources::SimResource;
use crate::ui::resources::{OpenContainer, TechnologyWindowState};

#[derive(SystemParam)]
pub(crate) struct ContainerOpenState<'w> {
    build_state: Res<'w, BuildPlacementState>,
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    sim: Res<'w, SimResource>,
    planner: Res<'w, PlannerState>,
    open_container: ResMut<'w, OpenContainer>,
}

pub(crate) fn handle_container_open_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: ContainerOpenState,
) {
    let Some(mouse) = mouse else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if world_input_blocked(state.input_state.as_deref())
        || !container_open_input_allowed(&state.build_state)
        || state.planner.tool != PlannerTool::None
    {
        return;
    }
    if state
        .technology_window
        .as_deref()
        .is_some_and(|window| window.open)
    {
        return;
    }
    if ui_buttons
        .iter()
        .any(|interaction| *interaction != Interaction::None)
    {
        return;
    }

    let cursor_tile = cursor_tile_from_window(&windows, &cameras);
    // Shift+click on a marked entity deconstructs it (see
    // `handle_ghost_click`) instead of opening its window.
    let shift_held = keyboard.as_deref().is_some_and(|keyboard| {
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
    });
    if shift_held
        && let Some((x, y)) = cursor_tile
        && let Some(entity_id) = state.sim.read().entities().occupancy().entity_at(x, y)
        && state
            .sim
            .read()
            .construction()
            .is_marked_for_deconstruction(entity_id)
    {
        return;
    }

    state.open_container.entity_id =
        opened_container_after_world_click(&state.sim.read(), cursor_tile);
}

pub(crate) fn handle_container_close_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    mut open_container: ResMut<OpenContainer>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if escape_consumed(input_state.as_deref()) {
        return;
    }
    if keyboard.just_pressed(KeyCode::Escape) {
        open_container.entity_id = None;
    }
}

pub fn opened_container_after_world_click(
    sim: &Simulation,
    cursor_tile: Option<(i32, i32)>,
) -> Option<EntityId> {
    let (x, y) = cursor_tile?;
    let entity_id = sim.entities().occupancy().entity_at(x, y)?;

    open_machine_kind(sim, entity_id)
        .is_some()
        .then_some(entity_id)
}

pub fn container_open_input_allowed(build_state: &BuildPlacementState) -> bool {
    build_state.selected.is_none()
}
