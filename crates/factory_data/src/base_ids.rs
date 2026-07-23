use crate::{EntityPrototypeId, FluidId, ItemId, PrototypeCatalog, RecipeId, TechnologyId, TileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BasePrototypeIds {
    pub items: BaseItemIds,
    pub fluids: BaseFluidIds,
    pub tiles: BaseTileIds,
}

impl BasePrototypeIds {
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            items: BaseItemIds::from_catalog(catalog),
            fluids: BaseFluidIds::from_catalog(catalog),
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
    pub burner_inserter: ItemId,
    pub fast_inserter: ItemId,
    pub long_handed_inserter: ItemId,
    pub transport_belt: ItemId,
    pub assembling_machine: ItemId,
    pub stone_furnace: ItemId,
    pub burner_mining_drill: ItemId,
    pub lab: ItemId,
    pub automation_science_pack: ItemId,
    pub logistic_science_pack: ItemId,
    pub chest: ItemId,
    pub iron_chest: ItemId,
    pub steel_chest: ItemId,
    pub stone_brick: ItemId,
    pub underground_belt: ItemId,
    pub splitter: ItemId,
    pub fast_transport_belt: ItemId,
    pub express_transport_belt: ItemId,
    pub fast_underground_belt: ItemId,
    pub express_underground_belt: ItemId,
    pub fast_splitter: ItemId,
    pub express_splitter: ItemId,
    pub small_electric_pole: ItemId,
    pub steam_engine: ItemId,
    pub boiler: ItemId,
    pub offshore_pump: ItemId,
    pub pipe: ItemId,
    pub pipe_to_ground: ItemId,
    pub pump: ItemId,
    pub storage_tank: ItemId,
    pub crude_oil: ItemId,
    pub pumpjack: ItemId,
    pub oil_refinery: ItemId,
    pub chemical_plant: ItemId,
    pub plastic_bar: ItemId,
    pub sulfur: ItemId,
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
            burner_inserter: item_id_by_name(catalog, "burner_inserter"),
            fast_inserter: item_id_by_name(catalog, "fast_inserter"),
            long_handed_inserter: item_id_by_name(catalog, "long_handed_inserter"),
            transport_belt: item_id_by_name(catalog, "transport_belt"),
            assembling_machine: item_id_by_name(catalog, "assembling_machine"),
            stone_furnace: item_id_by_name(catalog, "stone_furnace"),
            burner_mining_drill: item_id_by_name(catalog, "burner_mining_drill"),
            lab: item_id_by_name(catalog, "lab"),
            automation_science_pack: item_id_by_name(catalog, "automation_science_pack"),
            logistic_science_pack: item_id_by_name(catalog, "logistic_science_pack"),
            chest: item_id_by_name(catalog, "chest"),
            iron_chest: item_id_by_name(catalog, "iron_chest"),
            steel_chest: item_id_by_name(catalog, "steel_chest"),
            stone_brick: item_id_by_name(catalog, "stone_brick"),
            underground_belt: item_id_by_name(catalog, "underground_belt"),
            splitter: item_id_by_name(catalog, "splitter"),
            fast_transport_belt: item_id_by_name(catalog, "fast_transport_belt"),
            express_transport_belt: item_id_by_name(catalog, "express_transport_belt"),
            fast_underground_belt: item_id_by_name(catalog, "fast_underground_belt"),
            express_underground_belt: item_id_by_name(catalog, "express_underground_belt"),
            fast_splitter: item_id_by_name(catalog, "fast_splitter"),
            express_splitter: item_id_by_name(catalog, "express_splitter"),
            small_electric_pole: item_id_by_name(catalog, "small_electric_pole"),
            steam_engine: item_id_by_name(catalog, "steam_engine"),
            boiler: item_id_by_name(catalog, "boiler"),
            offshore_pump: item_id_by_name(catalog, "offshore_pump"),
            pipe: item_id_by_name(catalog, "pipe"),
            pipe_to_ground: item_id_by_name(catalog, "pipe_to_ground"),
            pump: item_id_by_name(catalog, "pump"),
            storage_tank: item_id_by_name(catalog, "storage_tank"),
            crude_oil: item_id_by_name(catalog, "crude_oil"),
            pumpjack: item_id_by_name(catalog, "pumpjack"),
            oil_refinery: item_id_by_name(catalog, "oil_refinery"),
            chemical_plant: item_id_by_name(catalog, "chemical_plant"),
            plastic_bar: item_id_by_name(catalog, "plastic_bar"),
            sulfur: item_id_by_name(catalog, "sulfur"),
        }
    }

    pub const fn resource_items(self) -> [ItemId; 4] {
        [self.iron_ore, self.copper_ore, self.coal, self.stone]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BaseFluidIds {
    pub water: FluidId,
    pub steam: FluidId,
    pub crude_oil: FluidId,
    pub petroleum_gas: FluidId,
}

impl BaseFluidIds {
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            water: fluid_id_by_name(catalog, "water"),
            steam: fluid_id_by_name(catalog, "steam"),
            crude_oil: fluid_id_by_name(catalog, "crude_oil"),
            petroleum_gas: fluid_id_by_name(catalog, "petroleum_gas"),
        }
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

pub fn fluid_id_by_name(catalog: &PrototypeCatalog, name: &str) -> FluidId {
    catalog
        .fluids
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required fluid prototype {name:?}"))
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
