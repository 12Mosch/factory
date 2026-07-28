use super::super::EntityVisualStyle;
use super::super::layers::{VisualLayerBuilder, direction_vec, is_horizontal, tinted};
use crate::constants::TILE_SIZE;
use bevy::prelude::*;
use factory_sim::Direction;

pub(super) fn transport_belt_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder
        .oriented(
            (Vec2::new(0.88, 0.13), Vec2::new(0.13, 0.88)),
            (Vec2::new(0.0, 0.23), Vec2::new(0.23, 0.0)),
            style.direction,
            0.08,
            Color::srgba(0.09, 0.075, 0.055, 0.64),
        )
        .oriented(
            (Vec2::new(0.88, 0.13), Vec2::new(0.13, 0.88)),
            (Vec2::new(0.0, -0.23), Vec2::new(-0.23, 0.0)),
            style.direction,
            0.08,
            Color::srgba(0.09, 0.075, 0.055, 0.64),
        )
        .oriented(
            (Vec2::new(0.84, 0.05), Vec2::new(0.05, 0.84)),
            (Vec2::ZERO, Vec2::ZERO),
            style.direction,
            0.09,
            Color::srgba(1.0, 0.88, 0.48, 0.36),
        );

    // Dark coupling bars bridge the sprite gap toward joined belt neighbors so lines of
    // belts read as one continuous run.
    for direction in Direction::ALL {
        if !style.connections.contains(direction) {
            continue;
        }
        let coupling_size = if is_horizontal(direction) {
            Vec2::new(0.12, 0.58)
        } else {
            Vec2::new(0.58, 0.12)
        };
        builder.scaled(
            coupling_size,
            direction_vec(direction) * 0.47,
            0.13,
            Color::srgba(0.07, 0.06, 0.045, 0.72),
        );
    }
}

pub(super) fn splitter_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    transport_belt_layers(builder, style);
    let port = Vec2::splat(TILE_SIZE * 0.22);
    let offset = if is_horizontal(style.direction) {
        Vec2::new(style.size.x * 0.30, 0.0)
    } else {
        Vec2::new(0.0, style.size.y * 0.30)
    };

    builder
        .oriented(
            (Vec2::new(0.12, 0.82), Vec2::new(0.82, 0.12)),
            (Vec2::ZERO, Vec2::ZERO),
            style.direction,
            0.14,
            Color::srgba(0.10, 0.08, 0.06, 0.70),
        )
        .rect(port, offset, 0.16, Color::srgba(0.95, 0.90, 0.68, 0.48))
        .rect(port, -offset, 0.16, Color::srgba(0.95, 0.90, 0.68, 0.48));
}

pub(super) fn chest_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.82, 0.18),
            Vec2::new(0.0, 0.18),
            0.10,
            tinted(style.base_color, 0.24),
            0.35,
        )
        .scaled_ellipse(
            Vec2::new(0.18, 0.30),
            Vec2::ZERO,
            0.12,
            Color::srgba(0.95, 0.74, 0.38, 0.72),
        );
}

pub(super) fn inserter_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let along = direction_vec(style.direction);
    builder
        .tile(
            Vec2::splat(0.44),
            Vec2::ZERO,
            0.08,
            Color::srgba(0.12, 0.10, 0.07, 0.68),
        )
        .oriented(
            (Vec2::new(0.64, 0.16), Vec2::new(0.16, 0.64)),
            (
                along * TILE_SIZE * 0.10 / style.size,
                along * TILE_SIZE * 0.10 / style.size,
            ),
            style.direction,
            0.12,
            Color::srgba(0.88, 0.72, 0.32, 0.86),
        )
        .tile(
            Vec2::splat(0.22),
            along * 0.34,
            0.14,
            Color::srgba(0.12, 0.10, 0.08, 0.76),
        );
}
