mod defense;
mod fluids;
mod infrastructure;
mod items;
mod logistics;
mod power;
mod production;

use bevy::prelude::*;
use factory_data::EntityKind;

use super::EntityVisualStyle;
use super::layers::{VisualLayer, VisualLayerBuilder};
use super::templates::VisualTemplate;
use crate::constants::TILE_SIZE;
use defense::*;
use fluids::*;
use infrastructure::*;
use items::*;
use logistics::*;
use power::*;
use production::*;

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

    // Pipes and heat pipes paint their own silhouette so arms only appear toward
    // connected neighbors.
    if matches!(style.kind, EntityKind::Pipe | EntityKind::HeatPipe) {
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
        EntityKind::Beacon => beacon_layers(&mut builder, style),
        EntityKind::Inserter => inserter_layers(&mut builder, style),
        EntityKind::ElectricPole => electric_pole_layers(&mut builder, style),
        EntityKind::SteamEngine => steam_engine_layers(&mut builder, style),
        EntityKind::Boiler => boiler_layers(&mut builder, style),
        EntityKind::OffshorePump => offshore_pump_layers(&mut builder, style),
        EntityKind::Pump => offshore_pump_layers(&mut builder, style),
        EntityKind::Pumpjack => pumpjack_layers(&mut builder, style),
        EntityKind::Pipe | EntityKind::HeatPipe => {}
        EntityKind::NuclearReactor => nuclear_reactor_layers(&mut builder, style),
        EntityKind::HeatExchanger => heat_exchanger_layers(&mut builder, style),
        EntityKind::StorageTank => storage_tank_layers(&mut builder, style),
        EntityKind::Wall => wall_layers(&mut builder, style),
        EntityKind::GunTurret => gun_turret_layers(&mut builder, style),
        EntityKind::LaserTurret => laser_turret_layers(&mut builder, style),
        EntityKind::EnemySpawner => enemy_spawner_layers(&mut builder, style),
        EntityKind::SolarPanel => solar_panel_layers(&mut builder, style),
        EntityKind::Accumulator => accumulator_layers(&mut builder, style),
        EntityKind::Radar => radar_layers(&mut builder, style),
        EntityKind::Roboport => roboport_layers(&mut builder, style),
        EntityKind::ConstantCombinator
        | EntityKind::ArithmeticCombinator
        | EntityKind::DeciderCombinator => combinator_layers(&mut builder, style),
        EntityKind::Lamp => lamp_layers(&mut builder, style),
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
    use factory_sim::Direction;

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
