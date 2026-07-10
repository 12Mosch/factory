use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::Direction;

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
        VisualTemplate::Entity {
            kind,
            direction,
            connections,
        } => entity_layers(EntityVisualStyle {
            base_color: color,
            size,
            kind,
            direction,
            connections,
        }),
        VisualTemplate::BeltItem => belt_item_layers(color, size),
        VisualTemplate::Resource => resource_layers(color, size),
    }
}

fn entity_layers(style: EntityVisualStyle) -> Vec<VisualLayer> {
    let mut builder = VisualLayerBuilder::new(style.size);

    // Pipes paint their own silhouette so arms only appear toward connected neighbors.
    if style.kind == EntityKind::Pipe {
        pipe_layers(&mut builder, style);
        return builder.finish();
    }

    shadow(&mut builder, style);
    entity_relief(&mut builder);

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
        EntityKind::Pipe => {}
        EntityKind::StorageTank => storage_tank_layers(&mut builder, style),
        EntityKind::Wall => wall_layers(&mut builder, style),
        EntityKind::GunTurret => gun_turret_layers(&mut builder, style),
        EntityKind::EnemySpawner => enemy_spawner_layers(&mut builder, style),
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

/// Pipes draw a hub plus one arm per joined neighbor instead of the standard full-tile
/// base, so runs read as plumbing. Arms reach the tile edge (past the sprite's base size)
/// to meet the neighbor's arm seamlessly.
fn pipe_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

fn wall_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

fn gun_turret_layers(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

fn enemy_spawner_layers(builder: &mut VisualLayerBuilder, _style: EntityVisualStyle) {
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

/// All entities share one key light from the top-left: a soft drop shadow cast toward the
/// bottom-right plus a tight contact shadow hugging the base so buildings sit on the ground.
fn shadow(builder: &mut VisualLayerBuilder, style: EntityVisualStyle) {
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

/// Outline plus edge relief matching the top-left key light: lit top and left edges,
/// shaded bottom and right edges.
fn entity_relief(builder: &mut VisualLayerBuilder) {
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

#[cfg(test)]
mod tests {
    use super::super::templates::ConnectionMask;
    use super::*;

    fn entity_template(kind: EntityKind, connections: ConnectionMask) -> VisualTemplate {
        VisualTemplate::Entity {
            kind,
            direction: Direction::North,
            connections,
        }
    }

    fn max_extent_x(layers: &[VisualLayer]) -> f32 {
        layers
            .iter()
            .map(|layer| layer.offset.x + layer.size.x * 0.5)
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn connected_pipe_grows_an_arm_reaching_the_tile_edge() {
        let size = Vec2::splat(TILE_SIZE * 0.92);
        let unconnected = visual_layers(
            entity_template(EntityKind::Pipe, ConnectionMask::EMPTY),
            Color::WHITE,
            size,
        );
        let east_connected = visual_layers(
            entity_template(
                EntityKind::Pipe,
                ConnectionMask::from_directions([false, true, false, false]),
            ),
            Color::WHITE,
            size,
        );

        assert!(max_extent_x(&east_connected) >= TILE_SIZE * 0.5 - 1e-4);
        assert!(max_extent_x(&unconnected) < TILE_SIZE * 0.5);
    }

    #[test]
    fn belt_connections_add_coupling_layers() {
        let size = Vec2::splat(TILE_SIZE * 0.92);
        let unconnected = visual_layers(
            entity_template(EntityKind::TransportBelt, ConnectionMask::EMPTY),
            Color::WHITE,
            size,
        );
        let joined = visual_layers(
            entity_template(
                EntityKind::TransportBelt,
                ConnectionMask::from_directions([true, false, true, false]),
            ),
            Color::WHITE,
            size,
        );

        assert_eq!(joined.len(), unconnected.len() + 2);
    }
}
