use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::MachineStatus;

use super::MapOverlayContext;
use super::entities::entity_footprint_is_visible;
use super::primitives::{MapOverlayPrimitive, spawn_point_overlay};
use crate::map::resources::MapOverlay;

pub(super) fn spawn_production_problem_overlays(
    overlays: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    if !context
        .settings
        .overlays
        .is_enabled(MapOverlay::ProductionProblems)
        || context.crop_bounds.width == 0
        || context.crop_bounds.height == 0
    {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(status) = context.sim.machine_status_for_entity(entity_id) else {
            continue;
        };
        if matches!(status, MachineStatus::Working | MachineStatus::Idle) {
            continue;
        }
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if context
            .sim
            .catalog()
            .entity(placed.prototype_id)
            .is_some_and(|prototype| {
                matches!(
                    prototype.entity_kind,
                    EntityKind::EnemySpawner | EntityKind::ResourcePatch
                )
            })
        {
            continue;
        }
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        let color = if status == MachineStatus::NoPower {
            Color::srgb(0.96, 0.12, 0.08)
        } else {
            Color::srgb(1.0, 0.58, 0.08)
        };
        let center = Vec2::new(
            placed.footprint.x as f32 + placed.footprint.width as f32 * 0.5,
            placed.footprint.y as f32 + placed.footprint.height as f32 * 0.5,
        );
        spawn_point_overlay(
            overlays,
            context.crop_bounds,
            context.image_size,
            center,
            7.0,
            color,
            Color::BLACK,
        );
    }
}
