use bevy::prelude::*;
use factory_sim::{CHUNK_SIZE, ThreatLocation};

use super::MapOverlayContext;
use super::primitives::{MapOverlayPrimitive, spawn_point_overlay, spawn_rect_overlay};
use crate::map::resources::MapOverlay;
use crate::ui::map_view::layout::map_rect_for_chunk;

pub(super) fn spawn_threat_overlays(
    overlays: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    if !context.settings.overlays.is_enabled(MapOverlay::Enemies) {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    let snapshot = context.sim.enemy_map_snapshot_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    );
    for coord in snapshot.contacted_sectors {
        if let Some(rect) = map_rect_for_chunk(context.crop_bounds, context.image_size, coord) {
            spawn_rect_overlay(
                overlays,
                rect,
                Color::srgba(1.0, 0.38, 0.18, 0.82),
                Color::srgba(0.8, 0.12, 0.06, 0.14),
                1.0,
            );
        }
    }
    for (_, x, y) in snapshot.known_bases {
        spawn_point_overlay(
            overlays,
            context.crop_bounds,
            context.image_size,
            Vec2::new(x as f32, y as f32),
            10.0,
            Color::srgb(0.95, 0.16, 0.08),
            Color::BLACK,
        );
    }
    for location in snapshot
        .raids
        .into_iter()
        .map(|(_, location)| location)
        .chain(
            snapshot
                .expansions
                .into_iter()
                .map(|(_, location)| location),
        )
    {
        let position = match location {
            ThreatLocation::Exact { x, y } => Vec2::new(x as f32, y as f32),
            ThreatLocation::Sector(coord) => {
                let (x, y) = coord.min_tile();
                Vec2::new(
                    (x + i64::from(CHUNK_SIZE) / 2) as f32,
                    (y + i64::from(CHUNK_SIZE) / 2) as f32,
                )
            }
        };
        spawn_point_overlay(
            overlays,
            context.crop_bounds,
            context.image_size,
            position,
            8.0,
            Color::srgb(1.0, 0.6, 0.12),
            Color::BLACK,
        );
    }
}
