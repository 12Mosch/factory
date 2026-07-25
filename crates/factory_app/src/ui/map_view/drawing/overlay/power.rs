use std::collections::BTreeMap;

use bevy::prelude::*;

use super::MapOverlayContext;
use super::entities::entity_footprint_is_visible;
use super::primitives::{MapOverlayPrimitive, spawn_point_overlay, spawn_world_line};
use crate::map::resources::MapOverlay;

pub(super) fn spawn_power_overlays(
    overlays: &mut Vec<MapOverlayPrimitive>,
    context: &MapOverlayContext,
) {
    if !context
        .settings
        .overlays
        .is_enabled(MapOverlay::PowerNetworks)
        || context.crop_bounds.width == 0
        || context.crop_bounds.height == 0
    {
        return;
    }
    let max_x = context.crop_bounds.min_x + i64::from(context.crop_bounds.width) - 1;
    let max_y = context.crop_bounds.min_y + i64::from(context.crop_bounds.height) - 1;
    let snapshot = context.sim.power_map_snapshot_in_tile_rect(
        context.crop_bounds.min_x,
        max_x,
        context.crop_bounds.min_y,
        max_y,
    );
    let poles = snapshot
        .poles
        .iter()
        .filter(|pole| {
            context
                .sim
                .entities()
                .placed_entity(pole.entity_id)
                .is_some_and(|placed| {
                    entity_footprint_is_visible(context.sim, context.settings, placed.footprint)
                })
        })
        .map(|pole| {
            (
                pole.entity_id,
                Vec2::new(pole.center_x2 as f32 * 0.5, pole.center_y2 as f32 * 0.5),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for connection in snapshot.connections {
        let (Some(start), Some(end)) = (
            poles.get(&connection.first_pole_id),
            poles.get(&connection.second_pole_id),
        ) else {
            continue;
        };
        let color = power_network_color(connection.network_id, connection.satisfaction_permyriad);
        spawn_world_line(
            overlays,
            context.crop_bounds,
            context.image_size,
            *start,
            *end,
            1.5,
            color,
        );
    }
    for pole in snapshot.poles {
        if !poles.contains_key(&pole.entity_id) {
            continue;
        }
        spawn_point_overlay(
            overlays,
            context.crop_bounds,
            context.image_size,
            Vec2::new(pole.center_x2 as f32 * 0.5, pole.center_y2 as f32 * 0.5),
            6.0,
            power_network_color(pole.network_id, pole.satisfaction_permyriad),
            Color::BLACK,
        );
    }
    for consumer in snapshot.consumers {
        if !entity_footprint_is_visible(context.sim, context.settings, consumer.footprint) {
            continue;
        }
        if consumer.network_id.is_none() {
            let center = Vec2::new(
                consumer.footprint.x as f32 + consumer.footprint.width as f32 * 0.5,
                consumer.footprint.y as f32 + consumer.footprint.height as f32 * 0.5,
            );
            spawn_point_overlay(
                overlays,
                context.crop_bounds,
                context.image_size,
                center,
                7.0,
                Color::srgb(0.95, 0.10, 0.08),
                Color::BLACK,
            );
        }
    }
}

fn power_network_color(network_id: u32, satisfaction_permyriad: u32) -> Color {
    if satisfaction_permyriad < 5_000 {
        return Color::srgb(0.95, 0.12, 0.08);
    }
    if satisfaction_permyriad < 9_500 {
        return Color::srgb(1.0, 0.58, 0.08);
    }
    const COLORS: [[f32; 3]; 8] = [
        [0.24, 0.78, 1.0],
        [0.38, 0.92, 0.52],
        [0.84, 0.52, 1.0],
        [1.0, 0.82, 0.26],
        [0.20, 0.90, 0.82],
        [0.96, 0.44, 0.68],
        [0.62, 0.72, 1.0],
        [0.72, 0.94, 0.30],
    ];
    let rgb = COLORS[network_id as usize % COLORS.len()];
    Color::srgb(rgb[0], rgb[1], rgb[2])
}
