//! F7 toggles the rail connectivity overlay.
//!
//! Track that looks continuous can still be two networks — a piece a tile off
//! the grid, a curve facing the wrong way — and nothing else on screen says so.
//! The overlay is the readout for that, so it gets a key of its own rather than
//! being tied to holding a rail.

use bevy::prelude::*;

use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::world_input_blocked;
use crate::input::resources::{AppInputState, RailGraphOverlay};

pub(crate) fn toggle_rail_overlay_from_input(
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    mut overlay: ResMut<RailGraphOverlay>,
) {
    if !actions.just_pressed(InputAction::ToggleRailOverlay)
        || world_input_blocked(input_state.as_deref())
    {
        return;
    }

    overlay.enabled = !overlay.enabled;
}
