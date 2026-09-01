use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use factory_sim::{EntityId, RollingStockId, Simulation};

use crate::build::resources::{BuildPlacementState, PlannerState, PlannerTool};
use crate::input::bindings::{ActionInput, InputAction};
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
    actions: ActionInput,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    ui_buttons: Query<&Interaction, With<Button>>,
    mut state: ContainerOpenState,
) {
    if !actions.just_pressed(InputAction::Primary) {
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
    let shift_held = actions.pressed(InputAction::Alternate);
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

    let (entity_id, stock_id) = opened_container_after_world_click(&state.sim.read(), cursor_tile);
    state.open_container.entity_id = entity_id;
    state.open_container.rolling_stock = stock_id;
}

pub(crate) fn handle_container_close_input(
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    mut open_container: ResMut<OpenContainer>,
) {
    if escape_consumed(input_state.as_deref()) {
        return;
    }
    if actions.just_pressed(InputAction::CancelPause) {
        open_container.close();
    }
}

/// What a click on `cursor_tile` opens: an entity's window, a piece of rolling
/// stock's, or neither. At most one is ever `Some`.
///
/// Rolling stock is checked first. A wagon stands *on* a rail, and the rail is
/// an ordinary placed entity, so the occupancy lookup would answer with the
/// track under the train — which opens nothing, and would leave a player
/// clicking a wagon with no window at all.
///
/// The stopped-stock index answers for a parked train in one lookup, which is
/// the case a player clicks by far the most often: trains are opened at
/// stations. The walk behind it is what catches a train still rolling, which
/// the index deliberately does not hold — a moving wagon can still be opened,
/// it simply cannot be reached by an inserter.
pub fn opened_container_after_world_click(
    sim: &Simulation,
    cursor_tile: Option<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord)>,
) -> (Option<EntityId>, Option<RollingStockId>) {
    let Some((x, y)) = cursor_tile else {
        return (None, None);
    };
    if let Some(stock_id) = sim.stopped_rolling_stock_at_tile(x, y).or_else(|| {
        sim.rolling_stock()
            .find(|stock| sim.rolling_stock_covers_tile(stock.id, x, y))
            .map(|stock| stock.id)
    }) {
        return (None, Some(stock_id));
    }

    let opened = sim
        .entities()
        .occupancy()
        .entity_at(x, y)
        .filter(|entity_id| open_machine_kind(sim, *entity_id).is_some());
    (opened, None)
}

pub fn container_open_input_allowed(build_state: &BuildPlacementState) -> bool {
    build_state.selected.is_none()
}
