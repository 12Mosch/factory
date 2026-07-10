use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{SimCommand, WorldTileCoord};

use crate::build::resources::{BuildPlacementState, PlannerState, PlannerTool};
use crate::input::panels::world_input_blocked;
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::{OpenContainer, TechnologyWindowState};

use super::build::technology_window_open;

pub(crate) const CLIPBOARD_BLUEPRINT_NAME: &str = "Clipboard";

/// A normalized drag-selected tile rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TileRect {
    pub min_x: WorldTileCoord,
    pub min_y: WorldTileCoord,
    pub max_x: WorldTileCoord,
    pub max_y: WorldTileCoord,
}

impl TileRect {
    pub(crate) fn from_corners(
        a: (WorldTileCoord, WorldTileCoord),
        b: (WorldTileCoord, WorldTileCoord),
    ) -> Self {
        Self {
            min_x: a.0.min(b.0),
            min_y: a.1.min(b.1),
            max_x: a.0.max(b.0),
            max_y: a.1.max(b.1),
        }
    }
}

fn planner_input_blocked(
    input_state: Option<&AppInputState>,
    technology_window: Option<&TechnologyWindowState>,
) -> bool {
    world_input_blocked(input_state) || technology_window_open(technology_window)
}

fn shift_pressed(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
}

pub(crate) fn control_pressed(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)
}

fn ui_button_hovered(ui_buttons: &Query<&Interaction, With<Button>>) -> bool {
    ui_buttons
        .iter()
        .any(|interaction| *interaction != Interaction::None)
}

#[derive(SystemParam)]
pub(crate) struct PlannerKeyState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    planner: ResMut<'w, PlannerState>,
    build_state: ResMut<'w, BuildPlacementState>,
    open_container: ResMut<'w, OpenContainer>,
}

/// Tool activation: Ctrl+C copies an area, Ctrl+V pastes the clipboard, and X
/// toggles the deconstruction planner. Escape deactivation lives in the panel
/// input chain so it cooperates with window priorities.
pub(crate) fn handle_planner_keys(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut state: PlannerKeyState,
) {
    if planner_input_blocked(
        state.input_state.as_deref(),
        state.technology_window.as_deref(),
    ) {
        return;
    }
    let Some(keyboard) = keyboard else {
        return;
    };

    let requested = if control_pressed(&keyboard) && keyboard.just_pressed(KeyCode::KeyC) {
        Some(PlannerTool::Copy)
    } else if control_pressed(&keyboard) && keyboard.just_pressed(KeyCode::KeyV) {
        state
            .planner
            .clipboard
            .is_some()
            .then_some(PlannerTool::Paste)
    } else if keyboard.just_pressed(KeyCode::KeyX) {
        if state.planner.tool == PlannerTool::Deconstruct {
            state.planner.set_tool(PlannerTool::None);
            return;
        }
        Some(PlannerTool::Deconstruct)
    } else {
        None
    };

    if let Some(tool) = requested {
        activate_planner_tool(
            &mut state.planner,
            &mut state.build_state,
            &mut state.open_container,
            tool,
        );
    }
}

/// Switches to a planner tool, clearing input modes that conflict with it.
pub(crate) fn activate_planner_tool(
    planner: &mut PlannerState,
    build_state: &mut BuildPlacementState,
    open_container: &mut OpenContainer,
    tool: PlannerTool,
) {
    planner.set_tool(tool);
    build_state.selected = None;
    build_state.last_status = Default::default();
    open_container.entity_id = None;
}

#[derive(SystemParam)]
pub(crate) struct PlannerDragState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    sim: Res<'w, SimResource>,
    planner: ResMut<'w, PlannerState>,
    build_state: ResMut<'w, BuildPlacementState>,
    commands: MessageWriter<'w, SimCommandRequest>,
}

/// Drag selection for the area tools (deconstruct, copy, capture blueprint):
/// press starts the selection, release applies it.
pub(crate) fn handle_planner_drag(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: PlannerDragState,
) {
    if planner_input_blocked(
        state.input_state.as_deref(),
        state.technology_window.as_deref(),
    ) {
        state.planner.drag_start = None;
        return;
    }
    if !matches!(
        state.planner.tool,
        PlannerTool::Deconstruct | PlannerTool::Copy | PlannerTool::CaptureBlueprint
    ) {
        state.planner.drag_start = None;
        return;
    }
    let (Some(keyboard), Some(mouse)) = (keyboard, mouse) else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left)
        && state.planner.drag_start.is_none()
        && !ui_button_hovered(&ui_buttons)
        && let Some(tile) = cursor_tile_from_window(&windows, &cameras)
    {
        state.planner.drag_start = Some(tile);
    }

    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let Some(drag_start) = state.planner.drag_start.take() else {
        return;
    };
    let end = cursor_tile_from_window(&windows, &cameras).unwrap_or(drag_start);
    let rect = TileRect::from_corners(drag_start, end);

    match state.planner.tool {
        PlannerTool::Deconstruct => {
            let command = if shift_pressed(&keyboard) {
                SimCommand::CancelDeconstruction {
                    min_x: rect.min_x,
                    min_y: rect.min_y,
                    max_x: rect.max_x,
                    max_y: rect.max_y,
                }
            } else {
                SimCommand::MarkDeconstruction {
                    min_x: rect.min_x,
                    min_y: rect.min_y,
                    max_x: rect.max_x,
                    max_y: rect.max_y,
                }
            };
            state.commands.write(SimCommandRequest(command));
        }
        PlannerTool::Copy => {
            match state.sim.read().capture_blueprint(
                CLIPBOARD_BLUEPRINT_NAME,
                rect.min_x,
                rect.min_y,
                rect.max_x,
                rect.max_y,
            ) {
                Ok(blueprint) => {
                    let count = blueprint.entities.len();
                    state.planner.clipboard = Some(blueprint);
                    state.planner.set_tool(PlannerTool::Paste);
                    state.build_state.last_status =
                        crate::build::resources::BuildPlacementStatus::Placed(format!(
                            "Copied {count} entities"
                        ));
                }
                Err(_) => {
                    state.planner.set_tool(PlannerTool::None);
                    state.build_state.last_status =
                        crate::build::resources::BuildPlacementStatus::CannotPlace(
                            "Nothing to copy".to_string(),
                        );
                }
            }
        }
        PlannerTool::CaptureBlueprint => {
            let name = format!(
                "Blueprint {}",
                state.sim.read().construction().blueprints().len() + 1
            );
            state
                .commands
                .write(SimCommandRequest(SimCommand::SaveBlueprint {
                    name,
                    min_x: rect.min_x,
                    min_y: rect.min_y,
                    max_x: rect.max_x,
                    max_y: rect.max_y,
                }));
            state.planner.set_tool(PlannerTool::None);
        }
        PlannerTool::None | PlannerTool::Paste => {}
    }
}

#[derive(SystemParam)]
pub(crate) struct PasteClickState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    planner: Res<'w, PlannerState>,
    commands: MessageWriter<'w, SimCommandRequest>,
}

/// Paste tool: click places the clipboard as ghosts with the blueprint origin
/// at the cursor tile. The tool stays active for repeated pastes.
pub(crate) fn handle_paste_click(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: PasteClickState,
) {
    if planner_input_blocked(
        state.input_state.as_deref(),
        state.technology_window.as_deref(),
    ) || state.planner.tool != PlannerTool::Paste
    {
        return;
    }
    let Some(mouse) = mouse else {
        return;
    };
    if !mouse.just_pressed(MouseButton::Left) || ui_button_hovered(&ui_buttons) {
        return;
    }
    let Some(blueprint) = state.planner.clipboard.as_ref() else {
        return;
    };
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    state
        .commands
        .write(SimCommandRequest(SimCommand::PasteBlueprint {
            entities: blueprint.entities.clone(),
            x,
            y,
        }));
}

#[derive(SystemParam)]
pub(crate) struct GhostClickState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    sim: Res<'w, SimResource>,
    planner: Res<'w, PlannerState>,
    build_state: Res<'w, BuildPlacementState>,
    commands: MessageWriter<'w, SimCommandRequest>,
}

/// Direct ghost interaction with an empty cursor: left-click builds a ghost
/// from the player inventory, right-click cancels it. Shift+left-click on a
/// marked entity deconstructs it immediately.
pub(crate) fn handle_ghost_click(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: GhostClickState,
) {
    if planner_input_blocked(
        state.input_state.as_deref(),
        state.technology_window.as_deref(),
    ) || state.planner.tool != PlannerTool::None
        || state.build_state.selected.is_some()
    {
        return;
    }
    let (Some(keyboard), Some(mouse)) = (keyboard, mouse) else {
        return;
    };
    let left = mouse.just_pressed(MouseButton::Left);
    let right = mouse.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    if ui_button_hovered(&ui_buttons) {
        return;
    }
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    if let Some(ghost) = state.sim.read().construction().ghost_at(x, y) {
        if left {
            state
                .commands
                .write(SimCommandRequest(SimCommand::BuildGhost {
                    ghost_id: ghost.id,
                }));
        } else {
            state
                .commands
                .write(SimCommandRequest(SimCommand::CancelGhost {
                    ghost_id: ghost.id,
                }));
        }
        return;
    }

    if left
        && shift_pressed(&keyboard)
        && let Some(entity_id) = state.sim.read().entities().occupancy().entity_at(x, y)
        && state
            .sim
            .read()
            .construction()
            .is_marked_for_deconstruction(entity_id)
    {
        state
            .commands
            .write(SimCommandRequest(SimCommand::DeconstructEntity {
                entity_id,
            }));
    }
}
