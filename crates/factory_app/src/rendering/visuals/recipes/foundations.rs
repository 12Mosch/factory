use bevy::prelude::*;

use crate::constants::TILE_SIZE;
use crate::rendering::visuals::EntityVisualStyle;
use crate::rendering::visuals::layers::VisualLayerBuilder;

/// Adds the soft cast and contact shadows that make an entity sit on the ground.
pub(super) fn shadow(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder
        .ellipse(
            style.size * Vec2::new(1.08, 1.08),
            Vec2::new(TILE_SIZE * 0.11, -TILE_SIZE * 0.11),
            -0.16,
            Color::srgba(0.015, 0.012, 0.010, 0.30),
        )
        .rounded_rect(
            style.size * Vec2::new(1.05, 1.05),
            Vec2::new(TILE_SIZE * 0.025, -TILE_SIZE * 0.04),
            -0.15,
            Color::srgba(0.02, 0.016, 0.012, 0.52),
            style.size.min_element() * 0.16,
        );
}

/// Adds edge relief matching the top-left key light.
pub(super) fn entity_relief(builder: &mut VisualLayerBuilder) {
    builder
        .scaled_rounded(
            Vec2::new(1.02, 1.02),
            Vec2::ZERO,
            -0.08,
            Color::srgba(0.035, 0.030, 0.026, 0.56),
            0.16,
        )
        .scaled_ellipse(
            Vec2::new(0.80, 0.12),
            Vec2::new(-0.02, 0.36),
            0.08,
            Color::srgba(1.0, 0.95, 0.72, 0.26),
        )
        .scaled_ellipse(
            Vec2::new(0.10, 0.62),
            Vec2::new(-0.38, 0.05),
            0.08,
            Color::srgba(1.0, 0.95, 0.72, 0.12),
        )
        .scaled_ellipse(
            Vec2::new(0.82, 0.12),
            Vec2::new(0.02, -0.37),
            0.08,
            Color::srgba(0.02, 0.02, 0.03, 0.24),
        )
        .scaled_ellipse(
            Vec2::new(0.10, 0.60),
            Vec2::new(0.38, -0.04),
            0.08,
            Color::srgba(0.02, 0.02, 0.03, 0.13),
        );
}
