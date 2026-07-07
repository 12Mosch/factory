use bevy::prelude::*;

use super::AppSet;
use crate::rendering::belts::{
    measured_sync_belt_direction_rendering, measured_sync_belt_item_rendering,
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
use crate::rendering::player::{measured_sync_player_sprite, spawn_player};
use crate::rendering::resources::{
    ResourceRenderCache, ResourceRenderSettings, measured_sync_resource_debug_rendering,
};
use crate::rendering::visuals::VisualAssetCache;
use crate::rendering::world::measured_sync_visible_world_tiles;
use crate::resources::{
    BeltItemRenderPool, RenderDetail, RenderSyncStats, VisibleChunks, VisibleEntityIds,
    WorldRenderCache,
};

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
            .add_systems(
                Startup,
                (
                    setup_camera,
                    spawn_player,
                    spawn_cursor_tile_highlight,
                    spawn_manual_mining_progress_bar,
                ),
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
                    update_visible_entity_ids.in_set(AppSet::VisibleEntities),
                    measured_sync_visible_world_tiles,
                    measured_sync_resource_debug_rendering,
                    measured_sync_placed_entity_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
                )
                    .chain()
                    .in_set(AppSet::RenderSync),
            );
    }
}
