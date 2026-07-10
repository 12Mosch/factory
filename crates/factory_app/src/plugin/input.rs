use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use super::AppSet;
use crate::input::camera::zoom_camera;
use crate::input::mining::update_manual_mining_from_input;
use crate::input::movement::move_player_from_input;
use crate::input::panels::{handle_panel_input, reset_app_input_state};
use crate::input::repair::update_repair_from_input;
use crate::input::resources::AppInputState;

/// Input resources, panel-state collection, and the fixed-step systems that
/// feed frame-collected input into the simulation.
pub(super) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<AppInputState>()
            .add_systems(
                PreUpdate,
                (reset_app_input_state, handle_panel_input)
                    .chain()
                    .in_set(AppSet::PanelInput),
            )
            .add_systems(
                FixedUpdate,
                (
                    move_player_from_input,
                    update_manual_mining_from_input,
                    update_repair_from_input,
                )
                    .chain()
                    .in_set(AppSet::SimInput),
            )
            .add_systems(Update, zoom_camera.in_set(AppSet::WorldInput));
    }
}
