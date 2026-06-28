use bevy::prelude::*;
use factory_data::{BasePrototypeIds, ItemId, PrototypeCatalog, TileId};
use factory_sim::ResourceCell;

pub(crate) fn chest_color() -> Color {
    Color::srgb(0.58, 0.42, 0.23)
}

pub(crate) fn burner_drill_color() -> Color {
    Color::srgb(0.40, 0.43, 0.40)
}

pub(crate) fn furnace_color() -> Color {
    Color::srgb(0.54, 0.45, 0.36)
}

pub(crate) fn assembler_color() -> Color {
    Color::srgb(0.28, 0.48, 0.56)
}

pub(crate) fn lab_color() -> Color {
    Color::srgb(0.47, 0.36, 0.62)
}

pub(crate) fn transport_belt_color(speed_subtiles_per_tick: Option<u16>) -> Color {
    match speed_subtiles_per_tick {
        Some(16) => Color::srgb(0.83, 0.24, 0.18),
        Some(24) => Color::srgb(0.18, 0.45, 0.88),
        _ => Color::srgb(0.93, 0.72, 0.18),
    }
}

pub(crate) fn splitter_color(speed_subtiles_per_tick: Option<u16>) -> Color {
    match speed_subtiles_per_tick {
        Some(16) => Color::srgb(0.68, 0.18, 0.15),
        Some(24) => Color::srgb(0.14, 0.34, 0.72),
        _ => Color::srgb(0.80, 0.54, 0.20),
    }
}

pub(crate) fn inserter_color() -> Color {
    Color::srgb(0.66, 0.58, 0.34)
}

pub(crate) fn tile_color(tile_id: TileId, ids: RenderPrototypeIds) -> Color {
    if tile_id == ids.water {
        Color::srgb(0.12, 0.34, 0.62)
    } else if tile_id == ids.dirt {
        Color::srgb(0.47, 0.38, 0.24)
    } else {
        Color::srgb(0.24, 0.50, 0.25)
    }
}

pub(crate) fn resource_color(resource: ResourceCell, ids: RenderPrototypeIds) -> Color {
    if resource.resource_item == ids.iron_ore {
        Color::srgb(0.62, 0.56, 0.50)
    } else if resource.resource_item == ids.copper_ore {
        Color::srgb(0.76, 0.36, 0.18)
    } else if resource.resource_item == ids.coal {
        Color::srgb(0.08, 0.08, 0.08)
    } else if resource.resource_item == ids.stone {
        Color::srgb(0.46, 0.43, 0.39)
    } else {
        Color::srgb(0.82, 0.78, 0.68)
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
        }
    }
}
