use bevy::diagnostic::{DiagnosticsPlugin, FrameCountPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::InputSystems;
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
use crate::input::panels::{handle_panel_input, reset_app_input_state};
use crate::interaction::container_open::{
    handle_container_close_input, handle_container_open_input,
};
use crate::rendering::belts::{
    measured_sync_belt_direction_rendering, measured_sync_belt_item_rendering,
};
use crate::rendering::build_preview::{
    spawn_build_preview, update_build_placement_preview_state, update_build_preview,
};
use crate::rendering::camera::{
    follow_player_camera, setup_camera, update_render_detail, update_visible_chunks,
};
use crate::rendering::entities::{
    measured_sync_placed_entity_rendering, update_visible_entity_ids,
};
use crate::rendering::manual_mining::{
    spawn_cursor_tile_highlight, spawn_manual_mining_progress_bar, update_cursor_tile_highlight,
    update_manual_mining_progress_bar,
};
use crate::rendering::map_texture::update_map_texture;
use crate::rendering::player::{measured_sync_player_sprite, spawn_player};
use crate::rendering::resources::{
    ResourceRenderCache, ResourceRenderSettings, measured_sync_resource_debug_rendering,
};
use crate::rendering::world::sync_visible_world_tiles;
use crate::resources::{
    AppInputState, BuildPlacementPreviewState, BuildPlacementState, CraftingWindowState,
    InventoryTransferFeedback, MapDisplaySettings, MapTextureCache, MapViewState, OpenContainer,
    ProductionStatsWindowState, RenderDetail, RenderSyncStats, SimProfileStats, SimResource,
    TechnologyWindowState, UpsStats, VisibleChunks, VisibleEntityIds, WorldRenderCache,
};
use crate::save_load::{
    AutosaveState, PendingSaveJobs, PresentationReloadToken, SaveLoadConfig, SaveLoadStatus,
    SaveLoadWindowState, handle_save_load_shortcuts, initialize_autosave_tick, poll_save_jobs,
    run_autosave,
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
use crate::ui::inventory_panel::{
    handle_container_slot_clicks, update_container_slot_text,
    update_inventory_transfer_feedback_text,
};
use crate::ui::machine_indicators::update_burner_drill_indicators;
use crate::ui::manual_crafting::{
    handle_manual_crafting_recipe_buttons, handle_manual_crafting_tab_buttons,
    sync_manual_crafting_panel,
};
use crate::ui::map_view::{sync_full_map_view, sync_minimap};
use crate::ui::production_stats::{handle_production_stats_buttons, sync_production_stats_window};
use crate::ui::save_load::{handle_save_load_buttons, sync_save_load_window};
use crate::ui::technology_panel::{
    ensure_selected_technology, handle_technology_panel_buttons, handle_technology_window_input,
    sync_technology_panel,
};

pub struct FactoryAppPlugin;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
enum AppInputSet {
    PanelInput,
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
            .init_resource::<BuildPlacementPreviewState>()
            .init_resource::<OpenContainer>()
            .init_resource::<InventoryTransferFeedback>()
            .init_resource::<TechnologyWindowState>()
            .init_resource::<CraftingWindowState>()
            .init_resource::<MapViewState>()
            .init_resource::<MapDisplaySettings>()
            .init_resource::<MapTextureCache>()
            .init_resource::<VisibleChunks>()
            .init_resource::<VisibleEntityIds>()
            .init_resource::<RenderDetail>()
            .init_resource::<WorldRenderCache>()
            .init_resource::<ProductionStatsWindowState>()
            .init_resource::<AppInputState>()
            .init_resource::<SaveLoadConfig>()
            .init_resource::<SaveLoadWindowState>()
            .init_resource::<SaveLoadStatus>()
            .init_resource::<PendingSaveJobs>()
            .init_resource::<AutosaveState>()
            .init_resource::<PresentationReloadToken>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                    setup_debug_overlay,
                    setup_build_bar,
                    spawn_build_preview,
                    initialize_autosave_tick,
                ),
            )
            .add_systems(
                PreUpdate,
                (reset_app_input_state, handle_panel_input)
                    .chain()
                    .in_set(AppInputSet::PanelInput)
                    .after(InputSystems)
                    .before(AppInputSet::TechnologyWindow),
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
                    update_build_placement_preview_state.before(update_build_preview),
                    update_build_preview,
                )
                    .in_set(AppInputSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    handle_save_load_shortcuts,
                    handle_save_load_buttons,
                    run_autosave,
                    poll_save_jobs,
                    sync_save_load_window,
                )
                    .chain()
                    .before(update_map_texture),
            )
            .add_systems(
                Update,
                (
                    update_map_texture,
                    update_render_detail,
                    update_visible_chunks,
                    update_visible_entity_ids,
                    sync_visible_world_tiles,
                    measured_sync_resource_debug_rendering,
                    measured_sync_placed_entity_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    update_debug_overlay,
                    sync_container_window,
                    handle_container_slot_clicks,
                    update_container_slot_text,
                    update_inventory_transfer_feedback_text
                        .after(sync_container_window)
                        .after(handle_container_slot_clicks),
                    update_build_bar_visuals,
                    update_build_bar_action_visuals,
                    update_build_status_text.after(update_build_placement_preview_state),
                    update_burner_drill_indicators,
                    sync_minimap.after(update_map_texture),
                    sync_full_map_view.after(update_map_texture),
                    handle_production_stats_buttons,
                    sync_production_stats_window.after(handle_production_stats_buttons),
                    handle_manual_crafting_tab_buttons,
                    handle_manual_crafting_recipe_buttons,
                    sync_manual_crafting_panel
                        .after(AppInputSet::PanelInput)
                        .after(handle_manual_crafting_tab_buttons)
                        .after(handle_manual_crafting_recipe_buttons),
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
