use super::super::EntityVisualStyle;
use super::super::layers::VisualLayerBuilder;
use bevy::prelude::*;

pub(super) fn electric_pole_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.20, 0.82),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.18, 0.12, 0.08, 0.82),
            0.45,
        )
        .scaled_rounded(
            Vec2::new(0.78, 0.14),
            Vec2::new(0.0, 0.20),
            0.12,
            Color::srgba(0.96, 0.82, 0.42, 0.72),
            0.45,
        );
}

pub(super) fn steam_engine_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.78, 0.24),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.70, 0.86, 0.90, 0.40),
            0.45,
        )
        .scaled_rounded(
            Vec2::new(0.18, 0.70),
            Vec2::new(0.28, 0.0),
            0.12,
            Color::srgba(0.12, 0.16, 0.17, 0.60),
            0.45,
        );
}

pub(super) fn boiler_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.72, 0.26),
            Vec2::new(0.0, -0.12),
            0.10,
            Color::srgba(0.96, 0.48, 0.16, 0.60),
            0.45,
        )
        .scaled_rounded(
            Vec2::new(0.68, 0.16),
            Vec2::new(0.0, 0.22),
            0.11,
            Color::srgba(0.20, 0.22, 0.22, 0.70),
            0.45,
        );
}

/// A containment ring around a glowing core, so a reactor reads as the heat source
/// at the center of a network rather than as another boiler.
pub(super) fn nuclear_reactor_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::splat(0.86),
            Vec2::ZERO,
            0.02,
            Color::srgba(0.16, 0.14, 0.15, 0.78),
            0.14,
        )
        .scaled_ellipse(
            Vec2::splat(0.58),
            Vec2::ZERO,
            0.08,
            Color::srgba(0.30, 0.22, 0.20, 0.86),
        )
        .scaled_ellipse(
            Vec2::splat(0.40),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.98, 0.62, 0.24, 0.80),
        )
        .scaled_ellipse(
            Vec2::splat(0.20),
            Vec2::ZERO,
            0.12,
            Color::srgba(1.0, 0.92, 0.66, 0.92),
        );
}

/// Heat in on one side, steam out the other: a hot inlet manifold beside a bank of
/// cool boiling tubes.
pub(super) fn heat_exchanger_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder.scaled_rounded(
        Vec2::new(0.26, 0.72),
        Vec2::new(-0.30, 0.0),
        0.09,
        Color::srgba(0.96, 0.52, 0.20, 0.72),
        0.35,
    );
    for offset in [-0.02, 0.16, 0.34] {
        builder.scaled_rounded(
            Vec2::new(0.16, 0.66),
            Vec2::new(offset, 0.0),
            0.11,
            Color::srgba(0.74, 0.84, 0.88, 0.74),
            0.40,
        );
    }
}

pub(super) fn solar_panel_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    // Dark metal frame with an inset photovoltaic cell grid.
    builder.scaled_rounded(
        Vec2::splat(0.94),
        Vec2::ZERO,
        0.02,
        Color::srgba(0.10, 0.13, 0.18, 0.82),
        0.18,
    );
    let cell = Vec2::splat(0.36);
    for (column, row) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        builder.scaled_rounded(
            cell,
            Vec2::new(column * 0.24, row * 0.24),
            0.05,
            Color::srgba(0.24, 0.52, 0.78, 0.90),
            0.14,
        );
    }
}

pub(super) fn accumulator_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    // Battery casing with two terminals and an internal charge bar.
    builder
        .scaled_rounded(
            Vec2::new(0.82, 0.86),
            Vec2::ZERO,
            0.02,
            Color::srgba(0.16, 0.24, 0.20, 0.86),
            0.22,
        )
        .scaled_rounded(
            Vec2::new(0.16, 0.12),
            Vec2::new(-0.22, -0.40),
            0.16,
            Color::srgba(0.86, 0.82, 0.42, 0.92),
            0.35,
        )
        .scaled_rounded(
            Vec2::new(0.16, 0.12),
            Vec2::new(0.22, -0.40),
            0.16,
            Color::srgba(0.86, 0.82, 0.42, 0.92),
            0.35,
        )
        .scaled_rounded(
            Vec2::new(0.52, 0.30),
            Vec2::new(0.0, 0.16),
            0.12,
            Color::srgba(0.40, 0.86, 0.58, 0.86),
            0.30,
        );
}
