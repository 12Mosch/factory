use bevy::prelude::*;

use super::MapOverlayContext;
use super::bounds::inclusive_max_tile;
use super::entities::entity_footprint_is_visible;
use super::primitives::{MapOverlayPrimitive, spawn_rect_overlay, spawn_ui_line};
use crate::map::resources::MapOverlay;
use crate::ui::map_view::layout::map_rect_for_footprint;

pub(super) fn spawn_construction_overlays(
    overlays: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    if !context
        .settings
        .overlays
        .is_enabled(MapOverlay::ConstructionPlans)
    {
        return;
    }
    let Some((max_x, max_y)) = inclusive_max_tile(context.crop_bounds) else {
        return;
    };
    let construction = context.sim.construction();
    for ghost_id in construction.ghost_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(ghost) = construction.ghost(ghost_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, ghost.footprint) {
            continue;
        }
        if let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, ghost.footprint)
        {
            spawn_rect_overlay(
                overlays,
                rect,
                Color::srgba(0.22, 0.70, 1.0, 0.95),
                Color::srgba(0.18, 0.58, 1.0, 0.16),
                1.5,
            );
        }
    }
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        if !construction.is_marked_for_deconstruction(entity_id) {
            continue;
        }
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        if let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, placed.footprint)
        {
            let red = Color::srgba(1.0, 0.18, 0.12, 0.95);
            spawn_rect_overlay(
                overlays,
                rect,
                red,
                Color::srgba(0.82, 0.08, 0.05, 0.10),
                1.5,
            );
            spawn_ui_line(
                overlays,
                Vec2::new(rect.left, rect.top),
                Vec2::new(rect.left + rect.width, rect.top + rect.height),
                1.5,
                red,
            );
            spawn_ui_line(
                overlays,
                Vec2::new(rect.left + rect.width, rect.top),
                Vec2::new(rect.left, rect.top + rect.height),
                1.5,
                red,
            );
        }
    }
}
