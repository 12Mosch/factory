use super::super::EntityVisualStyle;
use super::super::layers::{VisualLayerBuilder, direction_vec, tinted};
use bevy::prelude::*;

/// A landing pad ringed by four robot docks: the shape reads as somewhere
/// robots leave from, which is what distinguishes a roboport from the other
/// large square buildings at a glance.
pub(super) fn roboport_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::splat(0.86),
            Vec2::ZERO,
            0.09,
            tinted(style.base_color, -0.12),
            0.16,
        )
        .scaled_ellipse(
            Vec2::splat(0.50),
            Vec2::ZERO,
            0.12,
            Color::srgba(0.16, 0.20, 0.18, 0.92),
        )
        .scaled_ellipse(
            Vec2::splat(0.30),
            Vec2::ZERO,
            0.15,
            Color::srgba(0.86, 0.92, 0.84, 0.90),
        );
    for corner in [
        Vec2::new(-0.30, -0.30),
        Vec2::new(0.30, -0.30),
        Vec2::new(-0.30, 0.30),
        Vec2::new(0.30, 0.30),
    ] {
        builder.scaled_rounded(
            Vec2::splat(0.20),
            corner,
            0.14,
            Color::srgba(0.94, 0.78, 0.32, 0.92),
            0.35,
        );
    }
}

/// Flat housing with a display panel and two wire terminals. The terminals sit
/// on the entity's facing axis so an input-output combinator reads as directed
/// even though all three kinds share one silhouette.
pub(super) fn combinator_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let forward = direction_vec(style.direction);
    builder
        .scaled_rounded(
            Vec2::new(0.84, 0.88),
            Vec2::ZERO,
            0.0,
            Color::srgba(0.14, 0.16, 0.20, 0.92),
            0.16,
        )
        .scaled_rounded(
            Vec2::new(0.58, 0.40),
            Vec2::ZERO,
            0.0,
            tinted(style.base_color, 0.24),
            0.18,
        )
        .oriented(
            (Vec2::new(0.34, 0.10), Vec2::new(0.10, 0.34)),
            (forward * 0.36, forward * 0.36),
            style.direction,
            0.0,
            Color::srgba(0.82, 0.78, 0.44, 0.94),
        )
        .oriented(
            (Vec2::new(0.34, 0.10), Vec2::new(0.10, 0.34)),
            (forward * -0.36, forward * -0.36),
            style.direction,
            0.0,
            Color::srgba(0.52, 0.56, 0.62, 0.94),
        );
}

/// A lamp is a ring around a bulb; the lit state comes through `base_color`,
/// so the cached visual varies with it and no per-frame tinting is needed.
pub(super) fn lamp_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.86, 0.86),
            Vec2::ZERO,
            0.0,
            Color::srgba(0.20, 0.20, 0.22, 0.94),
            0.40,
        )
        .scaled_ellipse(
            Vec2::new(0.56, 0.56),
            Vec2::ZERO,
            0.0,
            tinted(style.base_color, 0.30),
        );
}

pub(super) fn radar_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    let forward = direction_vec(style.direction);
    let highlight_offset = builder.directional_offset(style.direction, Vec2::splat(0.30));
    builder
        .scaled_rounded(
            Vec2::new(0.76, 0.62),
            Vec2::new(0.0, -0.08),
            0.10,
            Color::srgba(0.20, 0.23, 0.21, 0.96),
            0.12,
        )
        .scaled_rounded(
            Vec2::new(0.56, 0.42),
            Vec2::new(0.0, 0.00),
            0.12,
            tinted(style.base_color, 0.12),
            0.14,
        )
        .oriented(
            (Vec2::new(0.50, 0.10), Vec2::new(0.10, 0.50)),
            (forward * 0.15, forward * 0.15),
            style.direction,
            0.14,
            Color::srgba(0.70, 0.75, 0.70, 0.96),
        )
        .scaled_ellipse(
            Vec2::new(0.19, 0.19),
            highlight_offset,
            0.16,
            Color::srgba(0.88, 0.72, 0.30, 0.98),
        )
        .oriented(
            (Vec2::new(0.46, 0.14), Vec2::new(0.14, 0.46)),
            (forward * 0.25, forward * 0.25),
            style.direction,
            0.18,
            Color::srgba(0.82, 0.87, 0.82, 0.98),
        )
        .oriented(
            (Vec2::new(0.08, 0.28), Vec2::new(0.28, 0.08)),
            (forward * 0.18, forward * 0.18),
            style.direction,
            0.19,
            Color::srgba(0.12, 0.15, 0.14, 0.94),
        );
}
