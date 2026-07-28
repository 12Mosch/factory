use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::map::resources::VisibleChunks;
use crate::rendering::belts::{
    measured_sync_belt_direction_rendering, measured_sync_belt_item_rendering,
};
use crate::rendering::camera::{
    follow_player_camera, setup_camera, update_render_detail, update_visible_chunks,
};
use crate::rendering::circuits::{CircuitWireRenderState, sync_circuit_wire_rendering};
use crate::rendering::day_night::{spawn_day_night_tint, sync_day_night_tint};
use crate::rendering::enemies::sync_enemy_rendering;
use crate::rendering::entities::{
    measured_sync_placed_entity_rendering, update_visible_entity_ids,
};
use crate::rendering::manual_mining::{
    spawn_cursor_tile_highlight, spawn_manual_mining_progress_bar, update_cursor_tile_highlight,
    update_manual_mining_progress_bar,
};
use crate::rendering::player::{measured_sync_player_sprite, spawn_player};
use crate::rendering::rail_graph::{
    RailGraphOverlay, sync_rail_graph_rendering, toggle_rail_graph_overlay,
};
use crate::rendering::resource_cells::{
    ResourceRenderCache, ResourceRenderSettings, measured_sync_resource_debug_rendering,
};
use crate::rendering::resources::{
    BeltItemRenderPool, RenderDetail, RenderSyncStats, VisibleEntityIds, WorldRenderCache,
};
use crate::rendering::robot_coverage::{
    RoboportCoverageRenderState, sync_roboport_coverage_rendering,
};
use crate::rendering::robots::sync_robot_rendering;
use crate::rendering::visuals::VisualAssetCache;
use crate::rendering::world::measured_sync_visible_world_tiles;

/// World presentation: camera, player sprite, and the chained render-sync
/// systems that mirror simulation state into render entities.
pub(super) struct RenderingPlugin;

impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderSyncStats>()
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
            .init_resource::<CircuitWireRenderState>()
            .init_resource::<RoboportCoverageRenderState>()
            .init_resource::<RailGraphOverlay>()
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                    spawn_day_night_tint,
                ),
            )
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
                    toggle_rail_graph_overlay,
                )
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    update_render_detail,
                    update_visible_chunks,
                    update_visible_entity_ids.in_set(AppSet::VisibleEntities),
                    measured_sync_visible_world_tiles,
                    measured_sync_resource_debug_rendering,
                    measured_sync_placed_entity_rendering,
                    sync_enemy_rendering,
                    sync_robot_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
                    sync_circuit_wire_rendering,
                    sync_roboport_coverage_rendering,
                    sync_rail_graph_rendering,
                )
                    .chain()
                    .in_set(AppSet::RenderSync),
            );
    }
}
