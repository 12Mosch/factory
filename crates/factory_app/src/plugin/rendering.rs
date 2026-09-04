use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::input::rail_debug::toggle_rail_overlay_from_input;
use crate::map::resources::VisibleChunks;
use crate::rendering::belts::{
    measured_sync_belt_direction_rendering, measured_sync_belt_item_rendering,
};
use crate::rendering::build_preview::update_build_placement_preview_state;
use crate::rendering::camera::{
    follow_player_camera, setup_camera, update_render_detail, update_visible_chunks,
};
use crate::rendering::circuits::{CircuitWireRenderState, sync_circuit_wire_rendering};
use crate::rendering::day_night::{spawn_day_night_tint, sync_day_night_tint};
use crate::rendering::enemies::sync_enemy_rendering;
use crate::rendering::entities::{
    measured_sync_placed_entity_rendering, sync_rocket_silo_rendering, update_visible_entity_ids,
};
use crate::rendering::manual_mining::{
    spawn_cursor_tile_highlight, spawn_manual_mining_progress_bar, update_cursor_tile_highlight,
    update_manual_mining_progress_bar,
};
use crate::rendering::player::{measured_sync_player_sprite, spawn_player};
use crate::rendering::rails::{
    RailOverlayRenderState, sync_rail_connection_preview, sync_rail_graph_overlay,
};
use crate::rendering::resource_cells::{
    ResourceRenderCache, ResourceRenderSettings, measured_sync_resource_debug_rendering,
};
use crate::rendering::resources::{
    BeltItemRenderPool, RenderDetail, VisibleEntityIds, WorldRenderCache,
    collect_render_sync_stats, init_render_sync_stats,
};
use crate::rendering::robot_coverage::{
    RoboportCoverageRenderState, sync_roboport_coverage_rendering,
};
use crate::rendering::robots::sync_robot_rendering;
use crate::rendering::rocket_launch::{RocketLaunchRenderPool, sync_rocket_launch_rendering};
use crate::rendering::rolling_stock::sync_rolling_stock_rendering;
use crate::rendering::visuals::VisualAssetCache;
use crate::rendering::world::measured_sync_visible_world_tiles;

/// World presentation: camera, player sprite, and the render-sync systems that
/// mirror simulation state into render entities.
pub(super) struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    /// Registers world presentation, deferring simulation-backed entities until world entry.
    fn build(&self, app: &mut App) {
        init_render_sync_stats(app)
            .insert_resource(ResourceRenderSettings {
                show_amount_labels: true,
            })
            .init_resource::<ResourceRenderCache>()
            .init_resource::<VisibleChunks>()
            .init_resource::<VisibleEntityIds>()
            .init_resource::<RenderDetail>()
            .init_resource::<VisualAssetCache>()
            .init_resource::<WorldRenderCache>()
            .init_resource::<BeltItemRenderPool>()
            .init_resource::<RocketLaunchRenderPool>()
            .init_resource::<CircuitWireRenderState>()
            .init_resource::<RoboportCoverageRenderState>()
            .init_resource::<RailOverlayRenderState>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                    spawn_day_night_tint,
                ),
            )
            .add_systems(OnEnter(crate::world_setup::AppMode::InGame), spawn_player)
            .add_systems(
                Update,
                sync_day_night_tint
                    .in_set(InGameSet)
                    .in_set(AppSet::RenderSync),
            )
            .add_systems(
                Update,
                (
                    measured_sync_player_sprite,
                    follow_player_camera,
                    update_cursor_tile_highlight,
                    update_manual_mining_progress_bar,
                )
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    update_render_detail,
                    update_visible_chunks,
                    update_visible_entity_ids
                        .after(update_visible_chunks)
                        .in_set(AppSet::VisibleEntities),
                    measured_sync_visible_world_tiles.after(update_visible_chunks),
                    measured_sync_resource_debug_rendering
                        .after(update_visible_chunks)
                        .after(update_render_detail),
                    measured_sync_placed_entity_rendering.after(update_visible_entity_ids),
                    sync_rocket_silo_rendering,
                    sync_rocket_launch_rendering.after(update_visible_entity_ids),
                    sync_enemy_rendering.after(update_visible_chunks),
                    sync_robot_rendering.after(update_visible_chunks),
                    sync_rolling_stock_rendering.after(update_visible_chunks),
                    measured_sync_belt_direction_rendering
                        .after(update_visible_entity_ids)
                        .after(update_render_detail),
                    measured_sync_belt_item_rendering
                        .after(update_visible_entity_ids)
                        .after(update_render_detail),
                    sync_circuit_wire_rendering.after(update_visible_chunks),
                    sync_roboport_coverage_rendering,
                    // Reads the overlay toggle written in `WorldInput`, which
                    // this set has no ordering against, so it says so itself
                    // rather than showing last frame's answer when the
                    // scheduler happens to run it first.
                    sync_rail_graph_overlay
                        .after(update_visible_chunks)
                        .after(toggle_rail_overlay_from_input),
                )
                    .in_set(AppSet::RenderSync),
            )
            // The rail connection markers follow the build cursor, so they
            // belong beside the build preview they annotate: same set, same
            // ordering against the state both of them read.
            .add_systems(
                Update,
                sync_rail_connection_preview
                    .after(update_build_placement_preview_state)
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                collect_render_sync_stats
                    .after(AppSet::RenderSync)
                    .after(measured_sync_player_sprite)
                    .in_set(InGameSet),
            );
    }
}
