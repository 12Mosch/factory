use super::super::EntityVisualStyle;
use super::super::layers::VisualLayerBuilder;
use bevy::prelude::*;

pub(super) fn drill_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder.scaled_rounded(
        Vec2::new(0.74, 0.24),
        Vec2::new(0.0, 0.18),
        0.10,
        Color::srgba(0.13, 0.15, 0.14, 0.72),
        0.45,
    );
    builder.rect(
        style.size * Vec2::new(0.22, 0.70),
        builder.directional_offset(style.direction, Vec2::splat(0.14)),
        0.12,
        Color::srgba(0.82, 0.68, 0.38, 0.84),
    );
    builder.rect(
        style.size * Vec2::new(0.40, 0.16),
        -builder.directional_offset(style.direction, Vec2::splat(0.20)),
        0.12,
        Color::srgba(0.10, 0.10, 0.09, 0.66),
    );
}

pub(super) fn furnace_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_ellipse(
            Vec2::new(0.46, 0.34),
            Vec2::new(0.0, -0.10),
            0.10,
            Color::srgba(0.95, 0.36, 0.12, 0.72),
        )
        .scaled_rounded(
            Vec2::new(0.18, 0.58),
            Vec2::new(0.26, 0.05),
            0.11,
            Color::srgba(0.13, 0.12, 0.11, 0.72),
            0.45,
        );
}

pub(super) fn assembler_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled(
            Vec2::new(0.66, 0.12),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.74, 0.88, 0.92, 0.50),
        )
        .scaled(
            Vec2::new(0.12, 0.66),
            Vec2::ZERO,
            0.11,
            Color::srgba(0.74, 0.88, 0.92, 0.50),
        )
        .scaled_ellipse(
            Vec2::splat(0.26),
            Vec2::ZERO,
            0.12,
            Color::srgba(0.09, 0.12, 0.13, 0.68),
        );
}

pub(super) fn lab_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_ellipse(
            Vec2::new(0.62, 0.38),
            Vec2::new(0.0, 0.04),
            0.10,
            Color::srgba(0.42, 0.86, 0.78, 0.52),
        )
        .scaled_rounded(
            Vec2::new(0.82, 0.10),
            Vec2::new(0.0, -0.28),
            0.11,
            Color::srgba(0.92, 0.80, 0.44, 0.42),
            0.45,
        );
}

pub(super) fn beacon_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_ellipse(
            Vec2::new(0.76, 0.30),
            Vec2::ZERO,
            0.09,
            Color::srgba(0.08, 0.22, 0.30, 0.78),
        )
        .scaled_ellipse(
            Vec2::new(0.54, 0.22),
            Vec2::ZERO,
            0.11,
            Color::srgba(0.22, 0.82, 0.96, 0.62),
        )
        .scaled_rounded(
            Vec2::new(0.12, 0.66),
            Vec2::new(0.0, 0.04),
            0.13,
            Color::srgba(0.72, 0.94, 1.0, 0.92),
            0.45,
        )
        .scaled_ellipse(
            Vec2::splat(0.18),
            Vec2::new(0.0, 0.30),
            0.15,
            Color::srgba(0.88, 0.98, 1.0, 0.98),
        );
}
