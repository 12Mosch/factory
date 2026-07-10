use bevy::prelude::*;
use factory_data::{BasePrototypeIds, InserterPrototype, ItemId, PrototypeCatalog, TileId};
use factory_sim::ResourceCell;

pub(crate) fn chest_color() -> Color {
    Color::srgb(0.64, 0.43, 0.22)
}

pub(crate) fn burner_drill_color() -> Color {
    Color::srgb(0.38, 0.44, 0.39)
}

pub(crate) fn furnace_color() -> Color {
    Color::srgb(0.55, 0.47, 0.38)
}

pub(crate) fn assembler_color() -> Color {
    Color::srgb(0.22, 0.50, 0.60)
}

pub(crate) fn oil_refinery_color() -> Color {
    Color::srgb(0.42, 0.38, 0.30)
}

pub(crate) fn chemical_plant_color() -> Color {
    Color::srgb(0.30, 0.55, 0.35)
}

pub(crate) fn pumpjack_color() -> Color {
    Color::srgb(0.35, 0.30, 0.28)
}

pub(crate) fn lab_color() -> Color {
    Color::srgb(0.48, 0.36, 0.66)
}

pub(crate) fn electric_pole_color() -> Color {
    Color::srgb(0.78, 0.61, 0.30)
}

pub(crate) fn steam_engine_color() -> Color {
    Color::srgb(0.40, 0.54, 0.57)
}

pub(crate) fn boiler_color() -> Color {
    Color::srgb(0.62, 0.35, 0.22)
}

pub(crate) fn offshore_pump_color() -> Color {
    Color::srgb(0.14, 0.48, 0.68)
}

pub(crate) fn pipe_color() -> Color {
    Color::srgb(0.50, 0.57, 0.58)
}

pub(crate) fn storage_tank_color() -> Color {
    Color::srgb(0.53, 0.62, 0.64)
}

pub(crate) fn wall_color() -> Color {
    Color::srgb(0.76, 0.78, 0.74)
}

pub(crate) fn gun_turret_color() -> Color {
    Color::srgb(0.62, 0.54, 0.30)
}

pub(crate) fn enemy_spawner_color() -> Color {
    Color::srgb(0.52, 0.22, 0.38)
}

pub(crate) fn enemy_unit_color() -> Color {
    Color::srgb(0.72, 0.26, 0.22)
}

pub(crate) fn transport_belt_color(speed_subtiles_per_tick: Option<u16>) -> Color {
    match speed_subtiles_per_tick {
        Some(16) => Color::srgb(0.82, 0.24, 0.16),
        Some(24) => Color::srgb(0.15, 0.42, 0.86),
        _ => Color::srgb(0.86, 0.61, 0.16),
    }
}

pub(crate) fn splitter_color(speed_subtiles_per_tick: Option<u16>) -> Color {
    match speed_subtiles_per_tick {
        Some(16) => Color::srgb(0.70, 0.20, 0.15),
        Some(24) => Color::srgb(0.13, 0.34, 0.72),
        _ => Color::srgb(0.76, 0.49, 0.16),
    }
}

pub(crate) fn inserter_color(inserter: Option<&InserterPrototype>) -> Color {
    match inserter {
        Some(inserter)
            if inserter
                .pickup_offset
                .x
                .abs()
                .max(inserter.pickup_offset.y.abs())
                > 1
                || inserter
                    .drop_offset
                    .x
                    .abs()
                    .max(inserter.drop_offset.y.abs())
                    > 1 =>
        {
            Color::srgb(0.88, 0.34, 0.14)
        }
        Some(inserter) if inserter.pickup_ticks <= 12 && inserter.drop_ticks <= 12 => {
            Color::srgb(0.17, 0.43, 0.86)
        }
        _ => Color::srgb(0.72, 0.60, 0.27),
    }
}

pub(crate) fn tile_color(tile_id: TileId, ids: RenderPrototypeIds) -> Color {
    if tile_id == ids.water {
        Color::srgb(0.08, 0.29, 0.54)
    } else if tile_id == ids.dirt {
        Color::srgb(0.42, 0.34, 0.23)
    } else {
        Color::srgb(0.22, 0.43, 0.24)
    }
}

pub(crate) fn resource_color(resource: ResourceCell, ids: RenderPrototypeIds) -> Color {
    if resource.resource_item == ids.iron_ore {
        Color::srgb(0.72, 0.66, 0.58)
    } else if resource.resource_item == ids.copper_ore {
        Color::srgb(0.86, 0.42, 0.20)
    } else if resource.resource_item == ids.coal {
        Color::srgb(0.10, 0.10, 0.095)
    } else if resource.resource_item == ids.stone {
        Color::srgb(0.54, 0.51, 0.46)
    } else if resource.resource_item == ids.crude_oil {
        Color::srgb(0.16, 0.10, 0.22)
    } else {
        Color::srgb(0.82, 0.78, 0.66)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RenderPrototypeIds {
    dirt: TileId,
    water: TileId,
    iron_ore: ItemId,
    copper_ore: ItemId,
    coal: ItemId,
    stone: ItemId,
    crude_oil: ItemId,
}

impl RenderPrototypeIds {
    pub(crate) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        let ids = BasePrototypeIds::from_catalog(catalog);
        Self {
            dirt: ids.tiles.dirt,
            water: ids.tiles.water,
            iron_ore: ids.items.iron_ore,
            copper_ore: ids.items.copper_ore,
            coal: ids.items.coal,
            stone: ids.items.stone,
            crude_oil: ids.items.crude_oil,
        }
    }
}
