use bevy::diagnostic::{DiagnosticsPlugin, FrameCountPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::time::Fixed;
use factory_data::PrototypeCatalog;
use factory_sim::Simulation;

use crate::constants::SIM_TICKS_PER_SECOND;
use crate::input::camera::zoom_camera;
use crate::input::debug_build::{
    handle_debug_belt_item_insertion_input, handle_debug_entity_placement,
};
use crate::input::debug_inventory::handle_debug_inventory_input;
use crate::input::mining::update_manual_mining_from_input;
use crate::input::movement::move_player_from_input;
use crate::interaction::container_open::{
    handle_container_close_input, handle_container_open_input,
};
use crate::rendering::belts::{
    measured_sync_belt_direction_rendering, measured_sync_belt_item_rendering,
};
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
    DebugBuildDirection, DebugInventorySelection, OpenContainer, RenderSyncStats, SimProfileStats,
    SimResource, UpsStats,
};
use crate::simulation::tick_sim;
use crate::ui::assembler_panel::{
    handle_assembler_recipe_button_clicks, update_assembler_detail_text,
    update_assembler_recipe_button_colors,
};
use crate::ui::container_window::sync_container_window;
use crate::ui::debug_overlay::{setup_debug_overlay, update_debug_overlay, update_ups_stats};
use crate::ui::inventory_panel::{handle_container_slot_clicks, update_container_slot_text};
use crate::ui::machine_indicators::update_burner_drill_indicators;

pub struct FactoryAppPlugin;

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
            .init_resource::<ResourceRenderSettings>()
            .init_resource::<ResourceRenderCache>()
            .init_resource::<DebugInventorySelection>()
            .init_resource::<OpenContainer>()
            .init_resource::<DebugBuildDirection>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_world_tiles,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                    setup_debug_overlay,
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
                    zoom_camera,
                    measured_sync_player_sprite,
                    follow_player_camera,
                    update_cursor_tile_highlight,
                    update_manual_mining_progress_bar,
                    update_ups_stats,
                    handle_debug_inventory_input,
                    handle_debug_entity_placement,
                    handle_debug_belt_item_insertion_input.after(handle_debug_inventory_input),
                    handle_container_open_input,
                    handle_container_close_input,
                    update_debug_overlay,
                    measured_sync_resource_debug_rendering,
                    measured_sync_placed_entity_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
                    sync_container_window,
                    handle_container_slot_clicks,
                    update_container_slot_text,
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
