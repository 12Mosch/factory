use serde::Deserialize;

use crate::model::{
    AssemblingMachinePrototype, BoilerPrototype, BurnerPrototype, CraftingCategory,
    ElectricEnergySourcePrototype, EntityKind, FluidBoxIo, FluidConnectionSide,
    OffshorePumpPrototype, SplitterPrototype, SteamEnginePrototype, TransportBeltPrototype,
};
use crate::validation::RawPrototype;

#[derive(Debug, Deserialize)]
pub(crate) struct RawPrototypeCatalog {
    pub(crate) items: Vec<RawItemPrototype>,
    #[serde(default)]
    pub(crate) fluids: Vec<RawFluidPrototype>,
    pub(crate) recipes: Vec<RawRecipePrototype>,
    pub(crate) entities: Vec<RawEntityPrototype>,
    pub(crate) tiles: Vec<RawTilePrototype>,
    #[serde(default)]
    pub(crate) technologies: Vec<RawTechnologyPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItemPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) stack_size: u16,
    pub(crate) fuel_value_joules: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawRecipePrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) category: CraftingCategory,
    pub(crate) crafting_time_ticks: u32,
    #[serde(default)]
    pub(crate) ingredients: Vec<RawItemAmount>,
    #[serde(default)]
    pub(crate) products: Vec<RawItemAmount>,
    #[serde(default)]
    pub(crate) fluid_ingredients: Vec<RawFluidAmount>,
    #[serde(default)]
    pub(crate) fluid_products: Vec<RawFluidAmount>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEntityPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) entity_kind: EntityKind,
    pub(crate) size: RawIVec2,
    pub(crate) collision_mask: RawCollisionMask,
    pub(crate) build_item: Option<String>,
    pub(crate) inventory_slot_count: Option<usize>,
    pub(crate) burner: Option<BurnerPrototype>,
    pub(crate) mining_drill: Option<RawMiningDrillPrototype>,
    pub(crate) assembling_machine: Option<AssemblingMachinePrototype>,
    pub(crate) transport_belt: Option<TransportBeltPrototype>,
    pub(crate) splitter: Option<SplitterPrototype>,
    pub(crate) inserter: Option<RawInserterPrototype>,
    pub(crate) electric_pole: Option<RawElectricPolePrototype>,
    pub(crate) electric_energy_source: Option<ElectricEnergySourcePrototype>,
    pub(crate) steam_engine: Option<SteamEnginePrototype>,
    pub(crate) boiler: Option<BoilerPrototype>,
    pub(crate) offshore_pump: Option<OffshorePumpPrototype>,
    pub(crate) pumpjack: Option<RawPumpjackPrototype>,
    #[serde(default)]
    pub(crate) fluid_boxes: Vec<RawFluidBoxPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawPumpjackPrototype {
    pub(crate) pumping_speed_per_second_milliunits: u64,
    pub(crate) resource_item: String,
    pub(crate) output_fluid: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidBoxPrototype {
    pub(crate) capacity_milliunits: u64,
    pub(crate) filter: Option<String>,
    #[serde(default)]
    pub(crate) io: FluidBoxIo,
    pub(crate) connections: Vec<RawFluidConnectionPrototype>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidConnectionPrototype {
    pub(crate) local_offset: RawIVec2,
    pub(crate) side: FluidConnectionSide,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMiningDrillPrototype {
    pub(crate) mining_area: RawIVec2,
    pub(crate) ticks_per_item: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawInserterPrototype {
    pub(crate) pickup_offset: RawIVec2,
    pub(crate) drop_offset: RawIVec2,
    pub(crate) pickup_ticks: u32,
    pub(crate) drop_ticks: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawElectricPolePrototype {
    pub(crate) supply_area_tiles: RawIVec2,
    pub(crate) wire_reach_tiles_x2: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTilePrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) collision_mask: RawCollisionMask,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTechnologyPrototype {
    pub(crate) id: u16,
    pub(crate) name: String,
    pub(crate) prerequisites: Vec<String>,
    pub(crate) science_packs: Vec<RawItemAmount>,
    pub(crate) required_units: u32,
    pub(crate) research_time_ticks: u32,
    pub(crate) effects: Vec<RawTechnologyEffect>,
}

#[derive(Debug, Deserialize)]
pub(crate) enum RawTechnologyEffect {
    UnlockRecipe(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawItemAmount {
    pub(crate) item: String,
    pub(crate) amount: u16,
}

/// Fluid quantity in whole fluid units; converted to milliunits on load.
#[derive(Debug, Deserialize)]
pub(crate) struct RawFluidAmount {
    pub(crate) fluid: String,
    pub(crate) amount: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawIVec2 {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCollisionMask {
    pub(crate) layers: Vec<String>,
}

impl RawPrototype for RawItemPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawRecipePrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawFluidPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawEntityPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawTilePrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl RawPrototype for RawTechnologyPrototype {
    fn id(&self) -> u16 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}
