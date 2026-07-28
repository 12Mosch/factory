use bevy::prelude::*;
use factory_data::EntityKind;

use super::defense::{enemy_spawner_layers, gun_turret_layers, laser_turret_layers, wall_layers};
use super::fluids::{offshore_pump_layers, pipe_layers, pumpjack_layers, storage_tank_layers};
use super::foundations::{entity_relief, shadow};
use super::infrastructure::{combinator_layers, lamp_layers, radar_layers, roboport_layers};
use super::logistics::{chest_layers, inserter_layers, splitter_layers, transport_belt_layers};
use super::power::{
    accumulator_layers, boiler_layers, electric_pole_layers, heat_exchanger_layers,
    nuclear_reactor_layers, solar_panel_layers, steam_engine_layers,
};
use super::production::{
    assembler_layers, beacon_layers, drill_layers, furnace_layers, lab_layers,
};
use super::rails::rail_layers;
use crate::rendering::visuals::EntityVisualStyle;
use crate::rendering::visuals::layers::{VisualLayer, VisualLayerBuilder};

pub(super) fn entity_layers(style: EntityVisualStyle) -> Vec<VisualLayer> {
    let mut builder = VisualLayerBuilder::new(style.size);

    // Pipes and heat pipes paint their own silhouette so arms only appear toward
    // connected neighbors.
    if matches!(style.kind, EntityKind::Pipe | EntityKind::HeatPipe) {
        pipe_layers(&mut builder, style);
        return builder.finish();
    }

    // Track is drawn along its own path rather than as a building block, so it
    // skips the shared body, shadow, and relief entirely.
    if let Some(rail) = style.rail {
        rail_layers(&mut builder, style, rail);
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
        EntityKind::OffshorePump | EntityKind::Pump => offshore_pump_layers(&mut builder, style),
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
        // Track without geometry cannot be drawn as track; it falls through to
        // the shared body below so a malformed prototype is visible rather than
        // invisible.
        EntityKind::RailStraight | EntityKind::RailCurved => {}
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
