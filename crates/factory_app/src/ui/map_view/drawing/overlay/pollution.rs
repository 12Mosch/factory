use bevy::prelude::*;
use factory_sim::ChunkCoord;

use super::MapOverlayContext;
use super::bounds::inclusive_max_tile;
use super::primitives::{MapOverlayPrimitive, spawn_rect_overlay};
use crate::map::resources::{MapOverlay, MapTextureBounds};
use crate::ui::map_view::layout::map_rect_for_chunk;

/// Red haze over polluted revealed chunks; opacity scales with the chunk's
/// pollution level.
pub(super) fn spawn_pollution_overlays(
    overlays: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    // Below this level the haze would be invisible anyway; skip the rect.
    const MIN_VISIBLE_POLLUTION_MICRO: u64 = 100_000;
    // Pollution level rendered at full haze opacity (10 pollution units).
    const FULL_HAZE_POLLUTION_MICRO: u64 = 10_000_000;

    if !context.settings.overlays.is_enabled(MapOverlay::Pollution) {
        return;
    }

    for coord in crop_chunk_coords(context.crop_bounds) {
        if !context.sim.world().chunks.contains_key(&coord) {
            continue;
        }
        let amount_micro = context.sim.pollution().amount_micro(coord);
        if amount_micro < MIN_VISIBLE_POLLUTION_MICRO {
            continue;
        }
        if !context.settings.debug_reveal_all && !context.sim.is_chunk_revealed(coord) {
            continue;
        }
        let Some(rect) = map_rect_for_chunk(context.crop_bounds, context.image_size, coord) else {
            continue;
        };

        let strength = ((amount_micro as f32 / MIN_VISIBLE_POLLUTION_MICRO as f32).ln_1p()
            / (FULL_HAZE_POLLUTION_MICRO as f32 / MIN_VISIBLE_POLLUTION_MICRO as f32).ln_1p())
        .clamp(0.06, 1.0);
        spawn_rect_overlay(
            overlays,
            rect,
            Color::NONE,
            Color::srgba(0.82, 0.20, 0.16, 0.30 * strength),
            0.0,
        );
    }
}

fn crop_chunk_coords(bounds: MapTextureBounds) -> impl Iterator<Item = ChunkCoord> {
    let Some((max_x, max_y)) = inclusive_max_tile(bounds) else {
        return Vec::new().into_iter();
    };
    let mut coords = Vec::new();
    if let (Some(min), Some(max)) = (
        ChunkCoord::from_tile(bounds.min_x, bounds.min_y),
        ChunkCoord::from_tile(max_x, max_y),
    ) {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                coords.push(ChunkCoord { x, y });
            }
        }
    }
    coords.into_iter()
}
