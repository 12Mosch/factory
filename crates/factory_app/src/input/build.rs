use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::audio::SoundEvent;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::placement::build::{
    entity_display_name, next_direction, place_selected_building_at_tile, short_inventory_need,
};
use crate::resources::{
    AppInputState, BuildPlacementState, BuildPlacementStatus, BuildSelection, HOTBAR_SLOT_COUNT,
    HotbarState, SimResource, TechnologyWindowState,
};

use super::panels::{escape_consumed, world_input_blocked};

#[derive(SystemParam)]
pub(crate) struct BuildWorldClickState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    sim: ResMut<'w, SimResource>,
    build_state: ResMut<'w, BuildPlacementState>,
    sounds: MessageWriter<'w, SoundEvent>,
}

pub(crate) fn handle_build_hotbar_keys(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    sim: Res<SimResource>,
    hotbar: Res<HotbarState>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if world_input_blocked(input_state.as_deref())
        || technology_window_open(technology_window.as_deref())
    {
        return;
    }
    let Some(keyboard) = keyboard else {
        return;
    };

    for (slot_index, key_code) in hotbar_keys().into_iter().enumerate() {
        if keyboard.just_pressed(key_code) {
            select_build_slot(
                &sim.sim,
                technology_window.as_deref(),
                &hotbar,
                &mut build_state,
                slot_index,
            );
            return;
        }
    }
}

pub(crate) fn handle_build_rotate_cancel_keys(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if world_input_blocked(input_state.as_deref())
        || escape_consumed(input_state.as_deref())
        || technology_window_open(technology_window.as_deref())
    {
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
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: BuildWorldClickState,
) {
    if world_input_blocked(state.input_state.as_deref())
        || technology_window_open(state.technology_window.as_deref())
    {
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

    let Some(selection) = state.build_state.selected else {
        return;
    };
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    let direction = state.build_state.direction;
    state.build_state.last_status =
        place_selected_building_at_tile(&mut state.sim.sim, selection, direction, x, y);
    match &state.build_state.last_status {
        BuildPlacementStatus::Placed(_) => {
            state.sounds.write(SoundEvent::Place);
        }
        BuildPlacementStatus::CannotPlace(_)
        | BuildPlacementStatus::MissingInventory(_)
        | BuildPlacementStatus::Locked(_) => {
            state.sounds.write(SoundEvent::PlaceError);
        }
        BuildPlacementStatus::Ready => {}
    }
    if state.sim.sim.player_inventory().count(selection.item_id) == 0 {
        state.build_state.selected = None;
    }
}

pub fn select_build_slot(
    sim: &factory_sim::Simulation,
    technology_window: Option<&TechnologyWindowState>,
    hotbar: &HotbarState,
    build_state: &mut BuildPlacementState,
    slot_index: usize,
) {
    if technology_window_open(technology_window) {
        return;
    }

    let Some(selection) = hotbar.slot(slot_index) else {
        build_state.selected = None;
        build_state.last_status = Default::default();
        return;
    };

    select_build_selection(sim, technology_window, build_state, selection);
}

/// Validates and applies a build selection. Returns whether the selection is
/// now active; on failure the selection is cleared and `last_status` explains
/// why.
pub fn select_build_selection(
    sim: &factory_sim::Simulation,
    technology_window: Option<&TechnologyWindowState>,
    build_state: &mut BuildPlacementState,
    selection: BuildSelection,
) -> bool {
    if technology_window_open(technology_window) {
        return false;
    }

    if !sim.is_entity_unlocked(selection.prototype_id) {
        build_state.selected = None;
        build_state.last_status = BuildPlacementStatus::Locked(format!(
            "{} locked",
            entity_display_name(sim.catalog(), selection.prototype_id)
                .unwrap_or_else(|| "Building".to_string())
        ));
        return false;
    }
    if sim.player_inventory().count(selection.item_id) == 0 {
        build_state.selected = None;
        build_state.last_status = BuildPlacementStatus::MissingInventory(short_inventory_need(
            sim.catalog(),
            selection.item_id,
        ));
        return false;
    }

    build_state.selected = Some(selection);
    build_state.last_status = Default::default();
    true
}

fn hotbar_keys() -> [KeyCode; HOTBAR_SLOT_COUNT] {
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
        KeyCode::Digit0,
    ]
}

pub(crate) fn technology_window_open(window: Option<&TechnologyWindowState>) -> bool {
    window.is_some_and(|state| state.open)
}
