use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::SimCommand;

use crate::build::resources::{
    BuildPlacementState, BuildPlacementStatus, BuildSelection, HOTBAR_SLOT_COUNT, HotbarState,
    PlannerState, PlannerTool,
};
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::placement::build::{entity_display_name, next_direction, short_inventory_need};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

use super::panels::{escape_consumed, world_input_blocked};

#[derive(SystemParam)]
pub(crate) struct BuildWorldClickState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    build_state: Res<'w, BuildPlacementState>,
    commands: MessageWriter<'w, SimCommandRequest>,
}

pub(crate) fn handle_build_hotbar_keys(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    input_state: Option<Res<AppInputState>>,
    technology_window: Option<Res<TechnologyWindowState>>,
    sim: Res<SimResource>,
    hotbar: Res<HotbarState>,
    mut build_state: ResMut<BuildPlacementState>,
    mut planner: ResMut<PlannerState>,
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
                &mut planner,
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
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
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

    // Shift-click plans a ghost instead of building immediately.
    let ghost = keyboard.as_deref().is_some_and(|keyboard| {
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
    });
    let command = if ghost {
        SimCommand::PlaceGhost {
            prototype_id: selection.prototype_id,
            x,
            y,
            direction: state.build_state.direction,
        }
    } else {
        SimCommand::PlaceEntityFromPlayerInventory {
            prototype_id: selection.prototype_id,
            item_id: selection.item_id,
            x,
            y,
            direction: state.build_state.direction,
        }
    };
    state.commands.write(SimCommandRequest(command));
}

pub fn select_build_slot(
    sim: &factory_sim::Simulation,
    technology_window: Option<&TechnologyWindowState>,
    hotbar: &HotbarState,
    build_state: &mut BuildPlacementState,
    planner: &mut PlannerState,
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

    select_build_selection(sim, technology_window, build_state, planner, selection);
}

/// Validates and applies a build selection. Returns whether the selection is
/// now active; on failure the selection is cleared and `last_status` explains
/// why. An empty inventory does not block selection: unlocked entities stay
/// selectable so shift-click can plan ghosts without the item. Activating a
/// selection deactivates any planner tool, keeping the two input modes
/// mutually exclusive.
pub fn select_build_selection(
    sim: &factory_sim::Simulation,
    technology_window: Option<&TechnologyWindowState>,
    build_state: &mut BuildPlacementState,
    planner: &mut PlannerState,
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

    build_state.selected = Some(selection);
    build_state.last_status = if sim.player_inventory().count(selection.item_id) == 0 {
        BuildPlacementStatus::MissingInventory(short_inventory_need(
            sim.catalog(),
            selection.item_id,
        ))
    } else {
        Default::default()
    };
    planner.set_tool(PlannerTool::None);
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
