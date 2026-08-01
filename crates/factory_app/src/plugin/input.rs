use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use super::AppSet;
use crate::input::camera::zoom_camera;
use crate::input::mining::update_manual_mining_from_input;
use crate::input::movement::move_player_from_input;
use crate::input::panels::{
    handle_build_menu_search_input, handle_panel_input, reset_app_input_state,
};
use crate::input::rail_debug::toggle_rail_overlay_from_input;
use crate::input::repair::update_repair_from_input;
use crate::input::resources::{AppInputState, RailGraphOverlay, TrainManualInput};
use crate::input::robot_debug::dispatch_debug_robot_from_input;
use crate::input::train_manual::{apply_train_manual_input, collect_train_manual_input};

/// Input resources, panel-state collection, and the fixed-step systems that
/// feed frame-collected input into the simulation.
pub(super) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<KeyboardInput>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<AppInputState>()
            .init_resource::<RailGraphOverlay>()
            .init_resource::<TrainManualInput>()
            .add_systems(
                PreUpdate,
                (
                    reset_app_input_state,
                    handle_panel_input,
                    handle_build_menu_search_input,
                )
                    .chain()
                    .in_set(AppSet::PanelInput),
            )
            .add_systems(
                FixedUpdate,
                (
                    move_player_from_input,
                    update_manual_mining_from_input,
                    update_repair_from_input,
                    dispatch_debug_robot_from_input,
                    apply_train_manual_input,
                )
                    .chain()
                    .in_set(AppSet::SimInput),
            )
            .add_systems(
                Update,
                (
                    zoom_camera,
                    toggle_rail_overlay_from_input,
                    collect_train_manual_input,
                )
                    .in_set(AppSet::WorldInput),
            );
    }
}
