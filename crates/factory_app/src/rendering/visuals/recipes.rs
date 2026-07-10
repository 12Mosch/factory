use bevy::prelude::*;
use factory_data::EntityKind;

use super::EntityVisualStyle;
use super::layers::{VisualLayer, VisualLayerBuilder, direction_vec, is_horizontal, tinted};
use super::templates::VisualTemplate;
use crate::constants::TILE_SIZE;

pub(super) fn visual_layers(
    template: VisualTemplate,
    color: Color,
    size: Vec2,
) -> Vec<VisualLayer> {
    match template {
        VisualTemplate::Entity { kind, direction } => entity_layers(EntityVisualStyle {
            base_color: color,
            size,
            kind,
            direction,
        }),
        VisualTemplate::BeltItem => belt_item_layers(color, size),
        VisualTemplate::Resource => resource_layers(color, size),
    }
}

fn entity_layers(style: EntityVisualStyle) -> Vec<VisualLayer> {
    let mut builder = VisualLayerBuilder::new(style.size);
    shadow(&mut builder, style);
    entity_outline(&mut builder);

    match style.kind {
        EntityKind::TransportBelt => transport_belt_layers(&mut builder, style),
        EntityKind::Splitter => splitter_layers(&mut builder, style),
        EntityKind::Chest => chest_layers(&mut builder, style),
        EntityKind::MiningDrill => drill_layers(&mut builder, style),
        EntityKind::Furnace => furnace_layers(&mut builder, style),
        EntityKind::AssemblingMachine => assembler_layers(&mut builder, style),
        EntityKind::Lab => lab_layers(&mut builder, style),
        EntityKind::Inserter => inserter_layers(&mut builder, style),
        EntityKind::ElectricPole => electric_pole_layers(&mut builder, style),
        EntityKind::SteamEngine => steam_engine_layers(&mut builder, style),
        EntityKind::Boiler => boiler_layers(&mut builder, style),
        EntityKind::OffshorePump => offshore_pump_layers(&mut builder, style),
        EntityKind::Pumpjack => pumpjack_layers(&mut builder, style),
        EntityKind::Pipe => pipe_layers(&mut builder, style),
        EntityKind::StorageTank => storage_tank_layers(&mut builder, style),
        EntityKind::ResourcePatch => {}
    }

    builder.rounded_rect(
        style.size,
        Vec2::ZERO,
        0.0,
        style.base_color,
        style.size.min_element() * 0.14,
    );
    builder.finish()
}

fn belt_item_layers(color: Color, size: Vec2) -> Vec<VisualLayer> {
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

fn resource_layers(color: Color, size: Vec2) -> Vec<VisualLayer> {
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

fn transport_belt_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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
}

fn splitter_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

fn chest_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

fn drill_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

fn furnace_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn assembler_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn lab_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn inserter_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

fn electric_pole_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn steam_engine_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn boiler_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn offshore_pump_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn pumpjack_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn pipe_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
    builder
        .scaled_rounded(
            Vec2::new(0.82, 0.18),
            Vec2::ZERO,
            0.10,
            Color::srgba(0.78, 0.90, 0.94, 0.40),
            0.50,
        )
        .scaled_rounded(
            Vec2::new(0.18, 0.82),
            Vec2::ZERO,
            0.11,
            Color::srgba(0.20, 0.24, 0.25, 0.38),
            0.50,
        );
}

fn storage_tank_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn shadow(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
    builder.ellipse(
        style.size * Vec2::new(1.04, 1.04),
        Vec2::new(TILE_SIZE * 0.06, -TILE_SIZE * 0.06),
        -0.16,
        Color::srgba(0.015, 0.012, 0.010, 0.46),
    );
}

fn entity_outline(builder: &mut VisualLayerBuilder) {
    builder
        .scaled_rounded(
            Vec2::new(1.02, 1.02),
            Vec2::ZERO,
            -0.08,
            Color::srgba(0.035, 0.030, 0.026, 0.56),
            0.16,
        )
        .scaled_ellipse(
            Vec2::new(0.78, 0.10),
            Vec2::new(0.0, 0.36),
            0.08,
            Color::srgba(1.0, 0.95, 0.72, 0.18),
        );
}
