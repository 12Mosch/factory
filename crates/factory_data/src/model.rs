use glam::IVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{EntityPrototypeId, ItemId, RecipeId, TechnologyId, TileId};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemPrototype {
    pub id: ItemId,
    pub name: String,
    pub stack_size: u16,
    pub fuel_value_joules: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RecipePrototype {
    pub id: RecipeId,
    pub name: String,
    pub category: CraftingCategory,
    pub crafting_time_ticks: u32,
    pub ingredients: Vec<ItemAmount>,
    pub products: Vec<ItemAmount>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityPrototype {
    pub id: EntityPrototypeId,
    pub name: String,
    pub entity_kind: EntityKind,
    pub size: IVec2,
    pub collision_mask: CollisionMask,
    pub build_item: Option<ItemId>,
    pub inventory_slot_count: Option<usize>,
    pub burner: Option<BurnerPrototype>,
    pub mining_drill: Option<MiningDrillPrototype>,
    pub assembling_machine: Option<AssemblingMachinePrototype>,
    pub transport_belt: Option<TransportBeltPrototype>,
    pub splitter: Option<SplitterPrototype>,
    pub inserter: Option<InserterPrototype>,
    pub electric_pole: Option<ElectricPolePrototype>,
    pub electric_energy_source: Option<ElectricEnergySourcePrototype>,
    pub steam_engine: Option<SteamEnginePrototype>,
    pub boiler: Option<BoilerPrototype>,
    pub offshore_pump: Option<OffshorePumpPrototype>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BurnerPrototype {
    pub energy_usage_watts: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MiningDrillPrototype {
    pub mining_area: IVec2,
    pub ticks_per_item: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AssemblingMachinePrototype {
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
    pub input_slot_count: usize,
    pub output_slot_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TransportBeltPrototype {
    pub speed_subtiles_per_tick: u16,
    pub underground: Option<UndergroundBeltPrototype>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SplitterPrototype {
    pub speed_subtiles_per_tick: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct InserterPrototype {
    pub pickup_offset: IVec2,
    pub drop_offset: IVec2,
    pub pickup_ticks: u32,
    pub drop_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricPolePrototype {
    pub supply_area_tiles: IVec2,
    pub wire_reach_tiles_x2: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricEnergySourcePrototype {
    pub energy_usage_watts: u64,
    pub drain_watts: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SteamEnginePrototype {
    pub max_power_output_watts: u64,
    pub steam_consumption_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BoilerPrototype {
    pub water_consumption_per_second_milliunits: u64,
    pub steam_output_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OffshorePumpPrototype {
    pub pumping_speed_per_second_milliunits: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundBeltPrototype {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum UndergroundBeltPart {
    Entrance,
    Exit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TilePrototype {
    pub id: TileId,
    pub name: String,
    pub collision_mask: CollisionMask,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TechnologyPrototype {
    pub id: TechnologyId,
    pub name: String,
    pub prerequisites: Vec<TechnologyId>,
    pub science_packs: Vec<ItemAmount>,
    pub required_units: u32,
    pub research_time_ticks: u32,
    pub effects: Vec<TechnologyEffect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum TechnologyEffect {
    UnlockRecipe(RecipeId),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemAmount {
    pub item: ItemId,
    pub amount: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CraftingCategory {
    Manual,
    Smelting,
    Crafting,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum EntityKind {
    ResourcePatch,
    Furnace,
    MiningDrill,
    AssemblingMachine,
    Inserter,
    TransportBelt,
    Splitter,
    Lab,
    Chest,
    ElectricPole,
    SteamEngine,
    Boiler,
    OffshorePump,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CollisionMask {
    pub layers: Vec<CollisionLayer>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CollisionLayer {
    Ground,
    Water,
    Resource,
    Building,
    Transport,
}
