use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::placement::build::{
    buildable_prototype_at_slot, next_direction, place_selected_building_at_tile,
    short_inventory_need,
};
use crate::resources::{BuildPlacementState, BuildSelection, SimResource, TechnologyWindowState};

pub(crate) fn handle_build_hotbar_keys(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    sim: Res<SimResource>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if technology_window_open(technology_window.as_deref()) {
        return;
    }
    let Some(keyboard) = keyboard else {
        return;
    };

    for (slot_index, key_code) in hotbar_keys().into_iter().enumerate() {
        if keyboard.just_pressed(key_code) {
            select_build_slot(&sim.sim, &mut build_state, slot_index);
            return;
        }
    }
}

pub(crate) fn handle_build_rotate_cancel_keys(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if technology_window_open(technology_window.as_deref()) {
        return;
    }
    let Some(keyboard) = keyboard else {
        return;
    };

    if keyboard.just_pressed(KeyCode::Escape) && build_state.selected.is_some() {
        build_state.selected = None;
        build_state.last_status = Default::default();
    }
    if keyboard.just_pressed(KeyCode::KeyR) && build_state.selected.is_some() {
        build_state.direction = next_direction(build_state.direction);
    }
}

pub(crate) fn handle_build_world_click(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut sim: ResMut<SimResource>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if technology_window_open(technology_window.as_deref()) {
        return;
    }
    let Some(mouse) = mouse else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if ui_buttons
        .iter()
        .any(|interaction| *interaction != Interaction::None)
    {
        return;
    }

    let Some(selection) = build_state.selected else {
        return;
    };
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    build_state.last_status =
        place_selected_building_at_tile(&mut sim.sim, selection, build_state.direction, x, y);
    if sim.sim.player_inventory().count(selection.item_id) == 0 {
        build_state.selected = None;
    }
}

pub fn select_build_slot(
    sim: &factory_sim::Simulation,
    build_state: &mut BuildPlacementState,
    slot_index: usize,
) {
    let Some(buildable) = buildable_prototype_at_slot(sim.catalog(), slot_index) else {
        build_state.selected = None;
        return;
    };

    if !sim.is_entity_unlocked(buildable.prototype_id) {
        build_state.selected = None;
        build_state.last_status = crate::resources::BuildPlacementStatus::Locked(format!(
            "{} locked",
            buildable.display_name
        ));
        return;
    }
    if sim.player_inventory().count(buildable.item_id) == 0 {
        build_state.selected = None;
        build_state.last_status = crate::resources::BuildPlacementStatus::MissingInventory(
            short_inventory_need(sim.catalog(), buildable.item_id),
        );
        return;
    }

    build_state.selected = Some(BuildSelection {
        prototype_id: buildable.prototype_id,
        item_id: buildable.item_id,
    });
    build_state.last_status = Default::default();
}

fn hotbar_keys() -> [KeyCode; 9] {
    [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ]
}

fn technology_window_open(window: Option<&TechnologyWindowState>) -> bool {
    window.is_some_and(|state| state.open)
}
