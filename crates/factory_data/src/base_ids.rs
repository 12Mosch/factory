use std::fmt;

use crate::{EntityPrototypeId, FluidId, ItemId, PrototypeCatalog, RecipeId, TechnologyId, TileId};

/// A prototype the engine hard-codes a dependency on that content data does not
/// define.
///
/// This is a content-data error rather than a programming error: the names come
/// from Rust, but whether they resolve is decided by the RON catalog that was
/// loaded. Catalog loading reports it as
/// [`PrototypeLoadError::MissingRequiredPrototype`], which keeps a bad or
/// incomplete data file a rejected load instead of a crash mid-game.
///
/// [`PrototypeLoadError::MissingRequiredPrototype`]: crate::PrototypeLoadError::MissingRequiredPrototype
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MissingBasePrototype {
    /// Prototype group the name was looked up in, e.g. `"item"`.
    pub group: &'static str,
    /// Name the engine requires and the catalog does not define.
    pub name: String,
}

impl fmt::Display for MissingBasePrototype {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { group, name } = self;
        write!(formatter, "missing required {group} prototype {name:?}")
    }
}

impl std::error::Error for MissingBasePrototype {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BasePrototypeIds {
    pub items: BaseItemIds,
    pub fluids: BaseFluidIds,
    pub tiles: BaseTileIds,
}

impl BasePrototypeIds {
    /// Resolves every base id the engine depends on.
    ///
    /// Catalog loading calls this to check a catalog up front, so the rest of
    /// the engine can resolve base ids without handling a missing name.
    pub fn try_from_catalog(catalog: &PrototypeCatalog) -> Result<Self, MissingBasePrototype> {
        Ok(Self {
            items: BaseItemIds::try_from_catalog(catalog)?,
            fluids: BaseFluidIds::try_from_catalog(catalog)?,
            tiles: BaseTileIds::try_from_catalog(catalog)?,
        })
    }

    /// Base ids of a catalog that already passed base-prototype validation.
    ///
    /// # Panics
    ///
    /// Panics if a required prototype is missing. Every catalog built by
    /// [`PrototypeCatalog::load_base`] or [`PrototypeCatalog::load_from_path`]
    /// has been checked for the full required set, so reaching this panic means
    /// a catalog was assembled without that check rather than that content data
    /// is bad.
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self::try_from_catalog(catalog).unwrap_or_else(|missing| panic!("{missing}"))
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
    pub landfill: ItemId,
    pub concrete: ItemId,
    pub red_wire: ItemId,
    pub green_wire: ItemId,
}

impl BaseItemIds {
    /// Resolves the base ids this build needs, reporting the first
    /// required prototype the catalog does not define.
    pub fn try_from_catalog(catalog: &PrototypeCatalog) -> Result<Self, MissingBasePrototype> {
        Ok(Self {
            iron_ore: try_item_id_by_name(catalog, "iron_ore")?,
            copper_ore: try_item_id_by_name(catalog, "copper_ore")?,
            coal: try_item_id_by_name(catalog, "coal")?,
            stone: try_item_id_by_name(catalog, "stone")?,
            iron_plate: try_item_id_by_name(catalog, "iron_plate")?,
            copper_plate: try_item_id_by_name(catalog, "copper_plate")?,
            steel_plate: try_item_id_by_name(catalog, "steel_plate")?,
            iron_gear_wheel: try_item_id_by_name(catalog, "iron_gear_wheel")?,
            copper_cable: try_item_id_by_name(catalog, "copper_cable")?,
            electronic_circuit: try_item_id_by_name(catalog, "electronic_circuit")?,
            inserter: try_item_id_by_name(catalog, "inserter")?,
            burner_inserter: try_item_id_by_name(catalog, "burner_inserter")?,
            fast_inserter: try_item_id_by_name(catalog, "fast_inserter")?,
            long_handed_inserter: try_item_id_by_name(catalog, "long_handed_inserter")?,
            transport_belt: try_item_id_by_name(catalog, "transport_belt")?,
            assembling_machine: try_item_id_by_name(catalog, "assembling_machine")?,
            stone_furnace: try_item_id_by_name(catalog, "stone_furnace")?,
            burner_mining_drill: try_item_id_by_name(catalog, "burner_mining_drill")?,
            lab: try_item_id_by_name(catalog, "lab")?,
            automation_science_pack: try_item_id_by_name(catalog, "automation_science_pack")?,
            logistic_science_pack: try_item_id_by_name(catalog, "logistic_science_pack")?,
            chest: try_item_id_by_name(catalog, "chest")?,
            iron_chest: try_item_id_by_name(catalog, "iron_chest")?,
            steel_chest: try_item_id_by_name(catalog, "steel_chest")?,
            stone_brick: try_item_id_by_name(catalog, "stone_brick")?,
            underground_belt: try_item_id_by_name(catalog, "underground_belt")?,
            splitter: try_item_id_by_name(catalog, "splitter")?,
            fast_transport_belt: try_item_id_by_name(catalog, "fast_transport_belt")?,
            express_transport_belt: try_item_id_by_name(catalog, "express_transport_belt")?,
            fast_underground_belt: try_item_id_by_name(catalog, "fast_underground_belt")?,
            express_underground_belt: try_item_id_by_name(catalog, "express_underground_belt")?,
            fast_splitter: try_item_id_by_name(catalog, "fast_splitter")?,
            express_splitter: try_item_id_by_name(catalog, "express_splitter")?,
            small_electric_pole: try_item_id_by_name(catalog, "small_electric_pole")?,
            steam_engine: try_item_id_by_name(catalog, "steam_engine")?,
            boiler: try_item_id_by_name(catalog, "boiler")?,
            offshore_pump: try_item_id_by_name(catalog, "offshore_pump")?,
            pipe: try_item_id_by_name(catalog, "pipe")?,
            pipe_to_ground: try_item_id_by_name(catalog, "pipe_to_ground")?,
            pump: try_item_id_by_name(catalog, "pump")?,
            storage_tank: try_item_id_by_name(catalog, "storage_tank")?,
            crude_oil: try_item_id_by_name(catalog, "crude_oil")?,
            pumpjack: try_item_id_by_name(catalog, "pumpjack")?,
            oil_refinery: try_item_id_by_name(catalog, "oil_refinery")?,
            chemical_plant: try_item_id_by_name(catalog, "chemical_plant")?,
            plastic_bar: try_item_id_by_name(catalog, "plastic_bar")?,
            sulfur: try_item_id_by_name(catalog, "sulfur")?,
            landfill: try_item_id_by_name(catalog, "landfill")?,
            concrete: try_item_id_by_name(catalog, "concrete")?,
            red_wire: try_item_id_by_name(catalog, "red_wire")?,
            green_wire: try_item_id_by_name(catalog, "green_wire")?,
        })
    }

    /// Base ids of a catalog that already passed base-prototype validation.
    ///
    /// # Panics
    ///
    /// Panics if a required prototype is missing. Every catalog built by
    /// [`PrototypeCatalog::load_base`] or
    /// [`PrototypeCatalog::load_from_path`] has been checked for the full
    /// required set, so reaching this panic means a catalog was assembled
    /// without that check rather than that content data is bad.
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self::try_from_catalog(catalog).unwrap_or_else(|missing| panic!("{missing}"))
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
    /// Resolves the base ids this build needs, reporting the first
    /// required prototype the catalog does not define.
    pub fn try_from_catalog(catalog: &PrototypeCatalog) -> Result<Self, MissingBasePrototype> {
        Ok(Self {
            water: try_fluid_id_by_name(catalog, "water")?,
            steam: try_fluid_id_by_name(catalog, "steam")?,
            crude_oil: try_fluid_id_by_name(catalog, "crude_oil")?,
            petroleum_gas: try_fluid_id_by_name(catalog, "petroleum_gas")?,
        })
    }

    /// Base ids of a catalog that already passed base-prototype validation.
    ///
    /// # Panics
    ///
    /// Panics if a required prototype is missing. Every catalog built by
    /// [`PrototypeCatalog::load_base`] or
    /// [`PrototypeCatalog::load_from_path`] has been checked for the full
    /// required set, so reaching this panic means a catalog was assembled
    /// without that check rather than that content data is bad.
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self::try_from_catalog(catalog).unwrap_or_else(|missing| panic!("{missing}"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BaseTileIds {
    pub grass: TileId,
    pub dirt: TileId,
    pub water: TileId,
    pub landfill: TileId,
    pub stone_path: TileId,
    pub concrete: TileId,
}

impl BaseTileIds {
    /// Resolves the base ids this build needs, reporting the first
    /// required prototype the catalog does not define.
    pub fn try_from_catalog(catalog: &PrototypeCatalog) -> Result<Self, MissingBasePrototype> {
        Ok(Self {
            grass: try_tile_id_by_name(catalog, "grass")?,
            dirt: try_tile_id_by_name(catalog, "dirt")?,
            water: try_tile_id_by_name(catalog, "water")?,
            landfill: try_tile_id_by_name(catalog, "landfill")?,
            stone_path: try_tile_id_by_name(catalog, "stone_path")?,
            concrete: try_tile_id_by_name(catalog, "concrete")?,
        })
    }

    /// Base ids of a catalog that already passed base-prototype validation.
    ///
    /// # Panics
    ///
    /// Panics if a required prototype is missing. Every catalog built by
    /// [`PrototypeCatalog::load_base`] or
    /// [`PrototypeCatalog::load_from_path`] has been checked for the full
    /// required set, so reaching this panic means a catalog was assembled
    /// without that check rather than that content data is bad.
    pub fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self::try_from_catalog(catalog).unwrap_or_else(|missing| panic!("{missing}"))
    }
}

pub fn try_item_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> Result<ItemId, MissingBasePrototype> {
    catalog
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .ok_or_else(|| MissingBasePrototype {
            group: "item",
            name: name.to_string(),
        })
}

/// # Panics
///
/// Panics if the catalog does not define `name`. Use [`try_item_id_by_name`] on
/// any path that resolves a name content data controls.
pub fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    try_item_id_by_name(catalog, name).unwrap_or_else(|missing| panic!("{missing}"))
}

pub fn try_fluid_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> Result<FluidId, MissingBasePrototype> {
    catalog
        .fluids
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .ok_or_else(|| MissingBasePrototype {
            group: "fluid",
            name: name.to_string(),
        })
}

/// # Panics
///
/// Panics if the catalog does not define `name`. Use [`try_fluid_id_by_name`] on
/// any path that resolves a name content data controls.
pub fn fluid_id_by_name(catalog: &PrototypeCatalog, name: &str) -> FluidId {
    try_fluid_id_by_name(catalog, name).unwrap_or_else(|missing| panic!("{missing}"))
}

pub fn try_tile_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> Result<TileId, MissingBasePrototype> {
    catalog
        .tiles
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .ok_or_else(|| MissingBasePrototype {
            group: "tile",
            name: name.to_string(),
        })
}

/// # Panics
///
/// Panics if the catalog does not define `name`. Use [`try_tile_id_by_name`] on
/// any path that resolves a name content data controls.
pub fn tile_id_by_name(catalog: &PrototypeCatalog, name: &str) -> TileId {
    try_tile_id_by_name(catalog, name).unwrap_or_else(|missing| panic!("{missing}"))
}

pub fn try_entity_prototype_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> Result<EntityPrototypeId, MissingBasePrototype> {
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .ok_or_else(|| MissingBasePrototype {
            group: "entity",
            name: name.to_string(),
        })
}

/// # Panics
///
/// Panics if the catalog does not define `name`. Use [`try_entity_prototype_id_by_name`] on
/// any path that resolves a name content data controls.
pub fn entity_prototype_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    try_entity_prototype_id_by_name(catalog, name).unwrap_or_else(|missing| panic!("{missing}"))
}

pub fn try_recipe_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> Result<RecipeId, MissingBasePrototype> {
    catalog
        .recipes
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .ok_or_else(|| MissingBasePrototype {
            group: "recipe",
            name: name.to_string(),
        })
}

/// # Panics
///
/// Panics if the catalog does not define `name`. Use [`try_recipe_id_by_name`] on
/// any path that resolves a name content data controls.
pub fn recipe_id_by_name(catalog: &PrototypeCatalog, name: &str) -> RecipeId {
    try_recipe_id_by_name(catalog, name).unwrap_or_else(|missing| panic!("{missing}"))
}

pub fn try_technology_id_by_name(
    catalog: &PrototypeCatalog,
    name: &str,
) -> Result<TechnologyId, MissingBasePrototype> {
    catalog
        .technologies
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .ok_or_else(|| MissingBasePrototype {
            group: "technology",
            name: name.to_string(),
        })
}

/// # Panics
///
/// Panics if the catalog does not define `name`. Use [`try_technology_id_by_name`] on
/// any path that resolves a name content data controls.
pub fn technology_id_by_name(catalog: &PrototypeCatalog, name: &str) -> TechnologyId {
    try_technology_id_by_name(catalog, name).unwrap_or_else(|missing| panic!("{missing}"))
}
