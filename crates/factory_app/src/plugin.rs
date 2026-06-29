use bevy::diagnostic::{DiagnosticsPlugin, FrameCountPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::time::Fixed;
use factory_data::PrototypeCatalog;
use factory_sim::Simulation;

use crate::constants::SIM_TICKS_PER_SECOND;
use crate::input::build::{
    handle_build_hotbar_keys, handle_build_rotate_cancel_keys, handle_build_world_click,
};
use crate::input::camera::zoom_camera;
use crate::input::mining::update_manual_mining_from_input;
use crate::input::movement::move_player_from_input;
use crate::interaction::container_open::{
    handle_container_close_input, handle_container_open_input,
};
use crate::rendering::belts::{
    measured_sync_belt_direction_rendering, measured_sync_belt_item_rendering,
};
use crate::rendering::build_preview::{spawn_build_preview, update_build_preview};
use crate::rendering::camera::{follow_player_camera, setup_camera};
use crate::rendering::entities::measured_sync_placed_entity_rendering;
use crate::rendering::manual_mining::{
    spawn_cursor_tile_highlight, spawn_manual_mining_progress_bar, update_cursor_tile_highlight,
    update_manual_mining_progress_bar,
};
use crate::rendering::player::{measured_sync_player_sprite, spawn_player};
use crate::rendering::resources::{
    ResourceRenderCache, ResourceRenderSettings, measured_sync_resource_debug_rendering,
};
use crate::rendering::world::spawn_world_tiles;
use crate::resources::{
    BuildPlacementState, OpenContainer, RenderSyncStats, SimProfileStats, SimResource,
    TechnologyWindowState, UpsStats,
};
use crate::simulation::tick_sim;
use crate::ui::assembler_panel::{
    handle_assembler_recipe_button_clicks, update_assembler_detail_text,
    update_assembler_recipe_button_colors,
};
use crate::ui::build_bar::{
    handle_build_bar_button_clicks, setup_build_bar, update_build_bar_action_visuals,
    update_build_bar_visuals, update_build_status_text,
};
use crate::ui::container_window::sync_container_window;
use crate::ui::debug_overlay::{setup_debug_overlay, update_debug_overlay, update_ups_stats};
use crate::ui::inventory_panel::{handle_container_slot_clicks, update_container_slot_text};
use crate::ui::machine_indicators::update_burner_drill_indicators;
use crate::ui::technology_panel::{
    ensure_selected_technology, handle_technology_panel_buttons, handle_technology_window_input,
    sync_technology_panel,
};

pub struct FactoryAppPlugin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
enum AppInputSet {
    TechnologyWindow,
    WorldInput,
}

impl Plugin for FactoryAppPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }
        if !app.is_plugin_added::<FrameCountPlugin>() {
            app.add_plugins(FrameCountPlugin);
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }

        app.insert_resource(Time::<Fixed>::from_hz(SIM_TICKS_PER_SECOND))
            .insert_resource(SimResource {
                sim: Simulation::new(
                    123,
                    PrototypeCatalog::load_base().expect("base prototype catalog should load"),
                ),
            })
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<UpsStats>()
            .init_resource::<SimProfileStats>()
            .init_resource::<RenderSyncStats>()
            .insert_resource(ResourceRenderSettings {
                show_amount_labels: true,
            })
            .init_resource::<ResourceRenderCache>()
            .init_resource::<BuildPlacementState>()
            .init_resource::<OpenContainer>()
            .init_resource::<TechnologyWindowState>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_world_tiles,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                    setup_debug_overlay,
                    setup_build_bar,
                    spawn_build_preview,
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    move_player_from_input,
                    update_manual_mining_from_input,
                    tick_sim,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    handle_technology_window_input,
                    ensure_selected_technology,
                    handle_technology_panel_buttons,
                    sync_technology_panel,
                )
                    .chain()
                    .in_set(AppInputSet::TechnologyWindow)
                    .before(AppInputSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    zoom_camera,
                    measured_sync_player_sprite,
                    follow_player_camera,
                    update_cursor_tile_highlight,
                    update_manual_mining_progress_bar,
                    update_ups_stats,
                    handle_build_hotbar_keys,
                    handle_build_rotate_cancel_keys.before(handle_container_close_input),
                    handle_build_bar_button_clicks,
                    handle_container_open_input.before(handle_build_world_click),
                    handle_build_world_click,
                    handle_container_close_input,
                    update_build_preview,
                )
                    .in_set(AppInputSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    measured_sync_resource_debug_rendering,
                    measured_sync_placed_entity_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
                ),
            )
            .add_systems(
                Update,
                (
                    update_debug_overlay,
                    sync_container_window,
                    handle_container_slot_clicks,
                    update_container_slot_text,
                    update_build_bar_visuals,
                    update_build_bar_action_visuals,
                    update_build_status_text,
                    update_burner_drill_indicators,
                ),
            )
            .add_systems(
                Update,
                (
                    handle_assembler_recipe_button_clicks.after(sync_container_window),
                    update_assembler_detail_text.after(sync_container_window),
                    update_assembler_recipe_button_colors.after(sync_container_window),
                ),
            );
    }
}
