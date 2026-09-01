use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::SimCommand;

use crate::build::resources::{
    BuildPlacementState, BuildPlacementStatus, BuildSelection, BuildTarget, HotbarState,
    PlannerState, PlannerTool,
};
use crate::input::bindings::{ActionInput, InputAction};
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
    actions: ActionInput,
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
    for (slot_index, action) in InputAction::HOTBAR.into_iter().enumerate() {
        if actions.just_pressed(action) {
            select_build_slot(
                &sim.read(),
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
    actions: ActionInput,
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
    if actions.just_pressed(InputAction::CancelPause) && build_state.selected.is_some() {
        build_state.selected = None;
        build_state.last_status = Default::default();
    }
    if actions.just_pressed(InputAction::RotateRepair) && build_state.selected.is_some() {
        build_state.direction = next_direction(build_state.direction);
    }
}

pub(crate) fn handle_build_world_click(
    actions: ActionInput,
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
    if !actions.just_pressed(InputAction::Primary) {
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

    // Shift-click plans a ghost instead of building immediately. Terrain has
    // no ghost form, so tile items always place immediately.
    let ghost = actions.pressed(InputAction::Alternate);
    let command = match selection.target {
        BuildTarget::Tile(_) => SimCommand::PlaceTileFromPlayerInventory {
            item_id: selection.item_id,
            x,
            y,
        },
        BuildTarget::Entity(prototype_id) if ghost => SimCommand::PlaceGhost {
            prototype_id,
            x,
            y,
            direction: state.build_state.direction,
        },
        BuildTarget::Entity(prototype_id) => SimCommand::PlaceEntityFromPlayerInventory {
            prototype_id,
            item_id: selection.item_id,
            x,
            y,
            direction: state.build_state.direction,
        },
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

    // Terrain items are gated by owning the item, not by an entity unlock.
    if let Some(prototype_id) = selection.entity_prototype_id()
        && !sim.is_entity_unlocked(prototype_id)
    {
        build_state.selected = None;
        build_state.last_status = BuildPlacementStatus::Locked(format!(
            "{} locked",
            entity_display_name(sim.catalog(), prototype_id)
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

pub(crate) fn technology_window_open(window: Option<&TechnologyWindowState>) -> bool {
    window.is_some_and(|state| state.open)
}
