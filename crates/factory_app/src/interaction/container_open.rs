use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{EntityId, Simulation};

use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::interaction::machine_kind::open_machine_kind;
use crate::resources::{BuildPlacementState, OpenContainer, SimResource};

pub(crate) fn handle_container_open_input(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    sim: Res<SimResource>,
    build_state: Res<BuildPlacementState>,
    mut open_container: ResMut<OpenContainer>,
) {
    let Some(mouse) = mouse else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if !container_open_input_allowed(&build_state) {
        return;
    }
    if ui_buttons
        .iter()
        .any(|interaction| *interaction != Interaction::None)
    {
        return;
    }

    open_container.entity_id =
        opened_container_after_world_click(&sim.sim, cursor_tile_from_window(&windows, &cameras));
}

pub(crate) fn handle_container_close_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut open_container: ResMut<OpenContainer>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
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
