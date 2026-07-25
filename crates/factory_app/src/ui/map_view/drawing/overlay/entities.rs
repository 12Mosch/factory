use bevy::prelude::*;
use factory_sim::{ChunkCoord, EntityFootprint, Simulation};

use super::MapOverlayContext;
use super::bounds::inclusive_max_tile;
use super::primitives::{MapOverlayPrimitive, spawn_rect_overlay};
use crate::map::resources::MapDisplaySettings;
use crate::rendering::entities::entity_prototype_render_style;
use crate::ui::map_view::layout::map_rect_for_footprint;

pub(super) fn spawn_entity_overlays(
    overlays: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    let Some((max_x, max_y)) = inclusive_max_tile(context.crop_bounds) else {
        return;
    };
    for entity_id in context.sim.entities().occupancy().entity_ids_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    ) {
        let Some(placed) = context.sim.entities().placed_entity(entity_id) else {
            continue;
        };
        if !entity_footprint_is_visible(context.sim, context.settings, placed.footprint) {
            continue;
        }
        let Some((color, _)) = entity_prototype_render_style(
            context.sim.catalog(),
            placed.prototype_id,
            placed.direction,
        ) else {
            continue;
        };
        let Some(rect) =
            map_rect_for_footprint(context.crop_bounds, context.image_size, placed.footprint)
        else {
            continue;
        };

        spawn_rect_overlay(
            overlays,
            rect,
            map_color_with_alpha(color, 0.96),
            map_color_with_alpha(color, 0.38),
            1.0,
        );
    }
}

pub(super) fn entity_footprint_is_visible(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    footprint: EntityFootprint,
) -> bool {
    settings.debug_reveal_all
        || footprint.tiles().into_iter().any(|(x, y)| {
            ChunkCoord::from_tile(x, y).is_some_and(|coord| sim.is_chunk_revealed(coord))
        })
}

fn map_color_with_alpha(color: Color, alpha: f32) -> Color {
    let srgba = color.to_srgba();
    Color::srgba(srgba.red, srgba.green, srgba.blue, alpha)
}
