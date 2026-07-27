use super::super::EntityVisualStyle;
use super::super::layers::{VisualLayerBuilder, direction_vec, is_horizontal, tinted};
use crate::constants::TILE_SIZE;
use bevy::prelude::*;
use factory_sim::Direction;

pub(super) fn offshore_pump_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_ellipse(
            Vec2::new(0.62, 0.32),
            Vec2::new(0.0, -0.12),
            0.10,
            Color::srgba(0.48, 0.82, 0.94, 0.58),
        )
        .scaled_rounded(
            Vec2::new(0.20, 0.72),
            Vec2::new(0.24, 0.0),
            0.11,
            Color::srgba(0.08, 0.16, 0.20, 0.58),
            0.45,
        );
}

pub(super) fn pumpjack_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.24, 0.72),
            Vec2::new(-0.18, 0.0),
            0.10,
            Color::srgba(0.10, 0.09, 0.08, 0.72),
            0.45,
        )
        .scaled_rounded(
            Vec2::new(0.66, 0.16),
            Vec2::new(0.06, 0.22),
            0.11,
            Color::srgba(0.88, 0.62, 0.24, 0.78),
            0.45,
        )
        .scaled_ellipse(
            Vec2::new(0.30, 0.26),
            Vec2::new(0.20, -0.16),
            0.12,
            Color::srgba(0.06, 0.05, 0.05, 0.66),
        );
}

/// Pipes draw a hub plus one arm per joined neighbor instead of the standard full-tile
/// base, so runs read as plumbing. Arms reach the tile edge (past the sprite's base size)
/// to meet the neighbor's arm seamlessly.
pub(super) fn pipe_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let unit = style.size.min_element();
    let reach = TILE_SIZE * 0.5;
    let arm_width = unit * 0.42;
    let rim = unit * 0.10;
    let outline = Color::srgba(0.035, 0.030, 0.026, 0.60);
    let light = Color::srgba(0.86, 0.94, 0.97, 0.42);
    let shade = Color::srgba(0.05, 0.08, 0.09, 0.40);
    let shadow_offset = Vec2::new(TILE_SIZE * 0.06, -TILE_SIZE * 0.06);

    // A straight run needs no joint hub: its two arms already form a continuous tube.
    let straight = style.connections.is_straight_run();

    if !straight {
        builder.ellipse(
            style.size * 0.72,
            shadow_offset * 1.4,
            -0.16,
            Color::srgba(0.015, 0.012, 0.010, 0.30),
        );
        builder.ellipse(
            style.size * 0.58,
            Vec2::new(TILE_SIZE * 0.02, -TILE_SIZE * 0.03),
            -0.15,
            Color::srgba(0.02, 0.016, 0.012, 0.44),
        );
    }

    for direction in Direction::ALL {
        if !style.connections.contains(direction) {
            continue;
        }
        let along = direction_vec(direction);
        let arm_offset = along * reach * 0.5;
        let (arm_size, arm_rim_size, stripe_size) = if is_horizontal(direction) {
            (
                Vec2::new(reach, arm_width),
                Vec2::new(reach, arm_width + rim),
                Vec2::new(reach, arm_width * 0.24),
            )
        } else {
            (
                Vec2::new(arm_width, reach),
                Vec2::new(arm_width + rim, reach),
                Vec2::new(arm_width * 0.24, reach),
            )
        };
        // Lit stripe toward the top-left of the arm, shaded stripe opposite.
        let stripe_offset = if is_horizontal(direction) {
            Vec2::new(0.0, arm_width * 0.28)
        } else {
            Vec2::new(-arm_width * 0.28, 0.0)
        };

        builder
            .rect(
                arm_rim_size,
                arm_offset + shadow_offset,
                -0.16,
                Color::srgba(0.02, 0.016, 0.012, 0.38),
            )
            .rect(arm_rim_size, arm_offset, -0.06, outline)
            .rect(arm_size, arm_offset, 0.0, style.base_color)
            .rect(stripe_size, arm_offset + stripe_offset, 0.10, light)
            .rect(stripe_size, arm_offset - stripe_offset, 0.10, shade);

        // Flange collar where the arm meets the neighbor.
        let flange_size = if is_horizontal(direction) {
            Vec2::new(unit * 0.12, arm_width + unit * 0.16)
        } else {
            Vec2::new(arm_width + unit * 0.16, unit * 0.12)
        };
        builder.rounded_rect(
            flange_size,
            along * (reach - unit * 0.10),
            0.12,
            tinted(style.base_color, 0.30),
            flange_size.min_element() * 0.40,
        );
    }

    if straight {
        return;
    }

    let hub = unit * 0.56;
    builder
        .rounded_rect(
            Vec2::splat(hub + rim),
            Vec2::ZERO,
            -0.05,
            outline,
            (hub + rim) * 0.30,
        )
        .rounded_rect(
            Vec2::splat(hub),
            Vec2::ZERO,
            0.02,
            style.base_color,
            hub * 0.30,
        )
        .ellipse(
            Vec2::new(hub * 0.58, hub * 0.30),
            Vec2::new(-hub * 0.08, hub * 0.20),
            0.11,
            light,
        )
        .ellipse(
            Vec2::new(hub * 0.52, hub * 0.24),
            Vec2::new(hub * 0.06, -hub * 0.22),
            0.11,
            shade,
        );

    // A sealed end cap keeps isolated pipes readable as pipes.
    if style.connections.is_empty() {
        builder
            .ellipse(
                Vec2::splat(hub * 0.50),
                Vec2::ZERO,
                0.12,
                Color::srgba(0.10, 0.13, 0.14, 0.55),
            )
            .ellipse(
                Vec2::splat(hub * 0.28),
                Vec2::ZERO,
                0.13,
                tinted(style.base_color, 0.18),
            );
    }
}

pub(super) fn storage_tank_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_ellipse(
            Vec2::new(0.74, 0.18),
            Vec2::new(0.0, 0.20),
            0.10,
            Color::srgba(0.78, 0.90, 0.92, 0.46),
        )
        .scaled_ellipse(
            Vec2::new(0.74, 0.18),
            Vec2::new(0.0, -0.20),
            0.10,
            Color::srgba(0.18, 0.23, 0.24, 0.42),
        )
        .scaled_rounded(
            Vec2::new(0.18, 0.76),
            Vec2::new(0.25, 0.0),
            0.11,
            Color::srgba(0.12, 0.16, 0.17, 0.42),
            0.45,
        );
}
