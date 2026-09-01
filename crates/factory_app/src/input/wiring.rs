//! Circuit wire tool: pick two connectors to join them, or right-click an
//! entity to cut every wire of the held color.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{
    CircuitNode, ConnectorPort, EntityId, SimCommand, Simulation, WireColor, WorldTileCoord,
};

use crate::build::resources::{
    BuildPlacementState, BuildPlacementStatus, PlannerState, PlannerTool,
};
use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::world_input_blocked;
use crate::input::planner::activate_planner_tool;
use crate::input::resources::AppInputState;
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::placement::build::display_name;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::{OpenContainer, TechnologyWindowState};

use super::build::technology_window_open;

#[derive(SystemParam)]
pub(crate) struct WireKeyState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    planner: ResMut<'w, PlannerState>,
    build_state: ResMut<'w, BuildPlacementState>,
    open_container: ResMut<'w, OpenContainer>,
}

/// Activates the selected wire tool, or puts it away when pressed again.
pub(crate) fn handle_wire_tool_keys(actions: ActionInput, mut state: WireKeyState) {
    if world_input_blocked(state.input_state.as_deref())
        || technology_window_open(state.technology_window.as_deref())
    {
        return;
    }
    let requested = if actions.just_pressed(InputAction::RedWire) {
        Some(WireColor::Red)
    } else if actions.just_pressed(InputAction::GreenWire) {
        Some(WireColor::Green)
    } else {
        None
    };
    let Some(color) = requested else {
        return;
    };

    if state.planner.tool == PlannerTool::Wire(color) {
        state.planner.set_tool(PlannerTool::None);
        return;
    }
    activate_planner_tool(
        &mut state.planner,
        &mut state.build_state,
        &mut state.open_container,
        PlannerTool::Wire(color),
    );
}

#[derive(SystemParam)]
pub(crate) struct WireClickState<'w> {
    input_state: Option<Res<'w, AppInputState>>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    sim: Res<'w, SimResource>,
    planner: ResMut<'w, PlannerState>,
    build_state: ResMut<'w, BuildPlacementState>,
    commands: MessageWriter<'w, SimCommandRequest>,
}

pub(crate) fn handle_wire_click(
    actions: ActionInput,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: WireClickState,
) {
    if world_input_blocked(state.input_state.as_deref())
        || technology_window_open(state.technology_window.as_deref())
    {
        return;
    }
    let PlannerTool::Wire(color) = state.planner.tool else {
        return;
    };
    let left = actions.just_pressed(InputAction::Primary);
    let right = actions.just_pressed(InputAction::Secondary);
    if !left && !right {
        return;
    }
    if ui_buttons
        .iter()
        .any(|interaction| *interaction != Interaction::None)
    {
        return;
    }
    let Some((x, y)) = cursor_tile_from_window(&windows, &cameras) else {
        return;
    };

    let cursor_node = {
        let sim = state.sim.read();
        connector_under_cursor(&sim, x, y, actions.pressed(InputAction::Alternate))
    };
    let Some(node) = cursor_node else {
        // Clicking empty ground cancels a half-drawn wire rather than leaving
        // a stale anchor pointing at something off screen.
        state.planner.wire_anchor = None;
        return;
    };

    if right {
        state.planner.wire_anchor = None;
        state
            .commands
            .write(SimCommandRequest(SimCommand::DisconnectAllCircuitWires {
                entity_id: node.entity_id,
                color,
            }));
        return;
    }

    let Some(anchor) = state.planner.wire_anchor else {
        state.planner.wire_anchor = Some(node);
        state.build_state.last_status = BuildPlacementStatus::Ready;
        return;
    };
    state.planner.wire_anchor = None;
    if anchor == node {
        return;
    }
    // Re-clicking the same entity through its other port is a self-connection
    // error, so surface it as a failed placement without sending a command.
    if anchor.entity_id == node.entity_id {
        let sim = state.sim.read();
        state.build_state.last_status = BuildPlacementStatus::CannotPlace(format!(
            "Cannot wire {} to itself",
            entity_name(&sim, node.entity_id)
        ));
        return;
    }

    state
        .commands
        .write(SimCommandRequest(SimCommand::ConnectCircuitWire {
            first: anchor,
            second: node,
            color,
        }));
}

/// Resolves the connector the cursor is over. Combinators expose two ports on
/// one footprint, so shift selects the output and a plain click the input —
/// the same modifier convention the build tools use for their alternate mode.
fn connector_under_cursor(
    sim: &Simulation,
    x: WorldTileCoord,
    y: WorldTileCoord,
    alternate: bool,
) -> Option<CircuitNode> {
    let entity_id = sim.entities().occupancy().entity_at(x, y)?;
    let connector = factory_sim::entity_access::circuit_connector(sim, entity_id)?;
    let port = match connector.ports {
        factory_data::CircuitPortLayout::Single => ConnectorPort::Single,
        factory_data::CircuitPortLayout::InputOutput => {
            if alternate {
                ConnectorPort::Output
            } else {
                ConnectorPort::Input
            }
        }
    };
    Some(CircuitNode::new(entity_id, port))
}

fn entity_name(sim: &Simulation, entity_id: EntityId) -> String {
    sim.entities()
        .placed_entity(entity_id)
        .and_then(|placed| sim.catalog().entity(placed.prototype_id))
        .map(|prototype| display_name(&prototype.name))
        .unwrap_or_else(|| "entity".to_string())
}
