use super::super::layers::{VisualLayer, VisualLayerBuilder};
use bevy::prelude::*;

pub(super) fn belt_item_layers(color: Color, size: Vec2) -> Vec<VisualLayer> {
    let mut builder = VisualLayerBuilder::new(size);
    builder
        .scaled(
            Vec2::new(1.12, 1.12),
            Vec2::new(0.10, -0.10),
            -0.10,
            Color::srgba(0.02, 0.018, 0.014, 0.42),
        )
        .scaled_rounded(Vec2::ONE, Vec2::ZERO, 0.0, color, 0.22)
        .scaled_ellipse(
            Vec2::new(0.72, 0.22),
            Vec2::new(0.0, 0.18),
            0.10,
            Color::srgba(1.0, 0.96, 0.78, 0.30),
        )
        .scaled_ellipse(
            Vec2::new(0.22, 0.70),
            Vec2::ZERO,
            0.12,
            Color::srgba(0.02, 0.02, 0.02, 0.24),
        );
    builder.finish()
}

pub(super) fn resource_layers(color: Color, size: Vec2) -> Vec<VisualLayer> {
    let mut builder = VisualLayerBuilder::new(size);
    builder
        .scaled(
            Vec2::new(1.18, 0.58),
            Vec2::new(0.08, -0.18),
            -0.08,
            Color::srgba(0.02, 0.018, 0.014, 0.38),
        )
        .scaled_ellipse(Vec2::ONE, Vec2::ZERO, 0.0, color)
        .scaled_ellipse(
            Vec2::new(0.52, 0.22),
            Vec2::new(-0.12, 0.16),
            0.08,
            Color::srgba(1.0, 0.94, 0.74, 0.25),
        );
    builder.finish()
}

pub(super) fn launch_rocket_layers(color: Color, size: Vec2) -> Vec<VisualLayer> {
    let mut builder = VisualLayerBuilder::new(size);
    builder
        .scaled_ellipse(
            Vec2::new(0.55, 0.28),
            Vec2::new(0.0, -0.46),
            -0.04,
            Color::srgba(1.0, 0.42, 0.12, 0.88),
        )
        .scaled_ellipse(
            Vec2::new(0.28, 0.18),
            Vec2::new(0.0, -0.58),
            -0.02,
            Color::srgba(1.0, 0.82, 0.32, 0.90),
        )
        .scaled_rounded(
            Vec2::new(0.62, 0.72),
            Vec2::new(0.0, -0.04),
            0.0,
            color,
            0.28,
        )
        .scaled(
            Vec2::new(0.18, 0.36),
            Vec2::new(-0.32, -0.22),
            0.04,
            Color::srgba(0.22, 0.24, 0.26, 0.92),
        )
        .scaled(
            Vec2::new(0.18, 0.36),
            Vec2::new(0.32, -0.22),
            0.04,
            Color::srgba(0.22, 0.24, 0.26, 0.92),
        )
        .scaled_ellipse(
            Vec2::new(0.46, 0.34),
            Vec2::new(0.0, 0.34),
            0.08,
            Color::srgba(0.88, 0.34, 0.24, 0.96),
        );
    builder.finish()
}
