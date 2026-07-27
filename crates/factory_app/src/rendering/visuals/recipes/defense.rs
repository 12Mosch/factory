use super::super::EntityVisualStyle;
use super::super::layers::{VisualLayerBuilder, direction_vec};
use bevy::prelude::*;

pub(super) fn wall_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled(
            Vec2::new(0.84, 0.30),
            Vec2::new(0.0, 0.22),
            0.10,
            Color::srgba(0.86, 0.88, 0.90, 0.34),
        )
        .scaled(
            Vec2::new(0.84, 0.12),
            Vec2::new(0.0, -0.06),
            0.11,
            Color::srgba(0.10, 0.11, 0.12, 0.40),
        )
        .scaled(
            Vec2::new(0.84, 0.12),
            Vec2::new(0.0, -0.30),
            0.11,
            Color::srgba(0.10, 0.11, 0.12, 0.40),
        );
}

pub(super) fn gun_turret_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let along = direction_vec(style.direction);
    builder
        .tile(
            Vec2::splat(0.58),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.16, 0.14, 0.10, 0.78),
        )
        .oriented(
            (Vec2::new(0.70, 0.16), Vec2::new(0.16, 0.70)),
            (along * 0.20, along * 0.20),
            style.direction,
            0.12,
            Color::srgba(0.30, 0.28, 0.22, 0.92),
        )
        .tile(
            Vec2::splat(0.30),
            Vec2::ZERO,
            0.14,
            Color::srgba(0.94, 0.78, 0.36, 0.72),
        );
}

pub(super) fn laser_turret_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let along = direction_vec(style.direction);
    builder
        .tile(
            Vec2::splat(0.62),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.05, 0.14, 0.22, 0.90),
        )
        .oriented(
            (Vec2::new(0.72, 0.20), Vec2::new(0.20, 0.72)),
            (along * 0.18, along * 0.18),
            style.direction,
            0.12,
            Color::srgba(0.14, 0.72, 0.92, 0.94),
        )
        .tile(
            Vec2::splat(0.36),
            Vec2::ZERO,
            0.14,
            Color::srgba(0.20, 0.88, 1.0, 0.88),
        )
        .tile(
            Vec2::splat(0.18),
            Vec2::ZERO,
            0.16,
            Color::srgba(0.78, 0.98, 1.0, 0.98),
        );
}

pub(super) fn enemy_spawner_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled(
            Vec2::new(0.86, 0.68),
            Vec2::new(0.0, -0.06),
            0.10,
            Color::srgba(0.46, 0.16, 0.34, 0.62),
        )
        .scaled(
            Vec2::new(0.38, 0.30),
            Vec2::new(0.0, 0.02),
            0.12,
            Color::srgba(0.90, 0.44, 0.62, 0.72),
        )
        .scaled(
            Vec2::new(0.16, 0.14),
            Vec2::new(-0.22, -0.20),
            0.13,
            Color::srgba(0.08, 0.04, 0.06, 0.70),
        )
        .scaled(
            Vec2::new(0.16, 0.14),
            Vec2::new(0.22, -0.20),
            0.13,
            Color::srgba(0.08, 0.04, 0.06, 0.70),
        );
}
