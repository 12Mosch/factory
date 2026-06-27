use bevy::prelude::*;
use factory_data::{EntityPrototypeId, ItemId, PrototypeCatalog, TileId};
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

pub(crate) fn transport_belt_color() -> Color {
    Color::srgb(0.93, 0.72, 0.18)
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
        Self {
            dirt: find_tile_id(catalog, "dirt"),
            water: find_tile_id(catalog, "water"),
            iron_ore: find_item_id(catalog, "iron_ore"),
            copper_ore: find_item_id(catalog, "copper_ore"),
            coal: find_item_id(catalog, "coal"),
            stone: find_item_id(catalog, "stone"),
        }
    }
}

pub(crate) fn find_tile_id(catalog: &PrototypeCatalog, name: &str) -> TileId {
    catalog
        .tiles
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required tile prototype {name:?}"))
}

pub(crate) fn find_item_id(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    catalog
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}

pub(crate) fn find_entity_prototype_id(
    catalog: &PrototypeCatalog,
    name: &str,
) -> EntityPrototypeId {
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
}
