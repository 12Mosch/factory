use crate::{EntityPrototypeId, ItemId, PrototypeCatalog, RecipeId, TechnologyId, TileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BasePrototypeIds {
    pub items: BaseItemIds,
    pub tiles: BaseTileIds,
}

impl BasePrototypeIds {
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            items: BaseItemIds::from_catalog(catalog),
            tiles: BaseTileIds::from_catalog(catalog),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BaseItemIds {
    pub iron_ore: ItemId,
    pub copper_ore: ItemId,
    pub coal: ItemId,
    pub stone: ItemId,
    pub iron_plate: ItemId,
    pub copper_plate: ItemId,
    pub steel_plate: ItemId,
    pub iron_gear_wheel: ItemId,
    pub copper_cable: ItemId,
    pub electronic_circuit: ItemId,
    pub inserter: ItemId,
    pub transport_belt: ItemId,
    pub assembling_machine: ItemId,
    pub stone_furnace: ItemId,
    pub burner_mining_drill: ItemId,
    pub lab: ItemId,
    pub automation_science_pack: ItemId,
    pub chest: ItemId,
    pub stone_brick: ItemId,
}

impl BaseItemIds {
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            iron_ore: item_id_by_name(catalog, "iron_ore"),
            copper_ore: item_id_by_name(catalog, "copper_ore"),
            coal: item_id_by_name(catalog, "coal"),
            stone: item_id_by_name(catalog, "stone"),
            iron_plate: item_id_by_name(catalog, "iron_plate"),
            copper_plate: item_id_by_name(catalog, "copper_plate"),
            steel_plate: item_id_by_name(catalog, "steel_plate"),
            iron_gear_wheel: item_id_by_name(catalog, "iron_gear_wheel"),
            copper_cable: item_id_by_name(catalog, "copper_cable"),
            electronic_circuit: item_id_by_name(catalog, "electronic_circuit"),
            inserter: item_id_by_name(catalog, "inserter"),
            transport_belt: item_id_by_name(catalog, "transport_belt"),
            assembling_machine: item_id_by_name(catalog, "assembling_machine"),
            stone_furnace: item_id_by_name(catalog, "stone_furnace"),
            burner_mining_drill: item_id_by_name(catalog, "burner_mining_drill"),
            lab: item_id_by_name(catalog, "lab"),
            automation_science_pack: item_id_by_name(catalog, "automation_science_pack"),
            chest: item_id_by_name(catalog, "chest"),
            stone_brick: item_id_by_name(catalog, "stone_brick"),
        }
    }

    pub const fn resource_items(self) -> [ItemId; 4] {
        [self.iron_ore, self.copper_ore, self.coal, self.stone]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BaseTileIds {
    pub grass: TileId,
    pub dirt: TileId,
    pub water: TileId,
}

impl BaseTileIds {
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            grass: tile_id_by_name(catalog, "grass"),
            dirt: tile_id_by_name(catalog, "dirt"),
            water: tile_id_by_name(catalog, "water"),
        }
    }
}

pub fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    catalog
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}

pub fn tile_id_by_name(catalog: &PrototypeCatalog, name: &str) -> TileId {
    catalog
        .tiles
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required tile prototype {name:?}"))
}

pub fn entity_prototype_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
}

pub fn recipe_id_by_name(catalog: &PrototypeCatalog, name: &str) -> RecipeId {
    catalog
        .recipes
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required recipe prototype {name:?}"))
}

pub fn technology_id_by_name(catalog: &PrototypeCatalog, name: &str) -> TechnologyId {
    catalog
        .technologies
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required technology prototype {name:?}"))
}
