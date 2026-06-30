pub(crate) use factory_data::{
    CraftingCategory, EntityKind, PrototypeCatalog, TechnologyEffect, TileId, UndergroundBeltPart,
};
use factory_data::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId};
use serde::{Deserialize, Serialize};
pub(crate) use smallvec::SmallVec;
pub(crate) use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::hash::{Hash, Hasher};

pub use crate::crafting::{CraftingError, CraftingJob, CraftingQueue};
pub(crate) use crate::entities::EntityReservation;
pub use crate::entities::{
    BuildError, Direction, EntityDestroyError, EntityFootprint, EntityStore, OccupancyGrid,
    PlacedEntity, PlayerBuildError, SimEntity,
};
pub use crate::fluids::{
    FluidBoxState, FluidConnectionPreview, FluidConnectionPreviewState, FluidNetworkBoxSnapshot,
    FluidNetworkSnapshot,
};
pub use crate::ids::{EntityId, Tick};
pub use crate::inventory::{Inventory, InventoryError, ItemStack};
pub use crate::logistics::{
    BeltError, BeltItem, BeltLane, BeltSegment, ContainerError, InserterError, InserterState,
    InserterTransferPreview, SplitterError, SplitterState, UndergroundBeltLinkPreview,
    UndergroundBeltSegment,
};
pub use crate::machines::{
    AssemblerError, AssemblerIngredientStatus, AssemblingMachineState, BoilerError, BoilerState,
    BurnerDrillError, BurnerEnergy, BurnerMiningDrillState, FurnaceError, FurnaceState, LabError,
    LabState, MachineStatus,
};
pub use crate::player::{ManualMiningProgress, ManualMiningTarget, PlayerState};
pub use crate::power::{
    ElectricConsumerState, ElectricPoleState, EntityPowerStatus, OffshorePumpState,
    PowerNetworkSnapshot, PowerSummary, SteamEngineState,
};
pub use crate::research::{
    ResearchError, ResearchProgressResult, ResearchState, TechnologyResearchState,
};
pub use crate::world::{
    Chunk, ChunkCoord, MinedResource, ResourceCell, ResourceTileChange, TileCell, TileCollision,
    WorldSim,
};

pub const CHUNK_SIZE: i32 = 32;
pub const PLAYER_MOVEMENT_SPEED_TILES_PER_SECOND: f32 = 5.0;
pub const PLAYER_MINING_SPEED: f32 = 0.5;
pub const ORE_MINING_TIME_SECONDS: f32 = 1.0;
pub const MANUAL_MINING_REACH_TILES: f32 = 2.5;
pub const MANUAL_MINING_TICKS_PER_ITEM: u32 =
    (ORE_MINING_TIME_SECONDS / PLAYER_MINING_SPEED * FIXED_SIM_TICKS_PER_SECOND) as u32;
pub const PLAYER_INVENTORY_SLOT_COUNT: usize = 80;
const FIXED_SIM_TICKS_PER_SECOND: f32 = 60.0;
pub const ITEM_STATISTICS_WINDOW_TICKS: u64 = 60 * FIXED_SIM_TICKS_PER_SECOND as u64;
const PLAYER_POSITION_SCALE: i64 = 1024;
const WORLD_MIN_CHUNK: i32 = -2;
const WORLD_MAX_CHUNK: i32 = 2;
const RESOURCE_PATCH_GRID_SIZE: i32 = 40;
const RESOURCE_PATCH_GRID_JITTER: i32 = 16;
const RESOURCE_PATCH_EDGE_NOISE: i32 = 3;
pub const BURNER_MINING_DRILL_FUEL_SLOT_INDEX: usize = 0;
pub const BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX: usize = 0;
pub const FURNACE_INPUT_SLOT_INDEX: usize = 0;
pub const FURNACE_FUEL_SLOT_INDEX: usize = 0;
pub const FURNACE_OUTPUT_SLOT_INDEX: usize = 0;
pub const BOILER_FUEL_SLOT_INDEX: usize = 0;
pub const ASSEMBLING_MACHINE_INPUT_SLOT_COUNT: usize = 4;
pub const ASSEMBLING_MACHINE_OUTPUT_SLOT_COUNT: usize = 1;
pub const BELT_SUBTILES_PER_TILE: u16 = 256;
pub const BELT_ITEM_SPACING_SUBTILES: u16 = 64;
pub const BASIC_INSERTER_PICKUP_TICKS: u32 = 35;
pub const BASIC_INSERTER_DROP_TICKS: u32 = 35;
pub const POWER_SATISFACTION_FULL_PERMYRIAD: u32 = 10_000;
const FIXED_SIM_TICKS_PER_SECOND_F64: f64 = 60.0;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Simulation {
    tick: u64,
    world: WorldSim,
    chart: ChartState,
    item_statistics: ItemStatistics,
    entities: EntityStore,
    player: PlayerState,
    player_inventory: Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    pub research: ResearchState,
    power_summary: PowerSummary,
    power_networks: Vec<PowerNetworkSnapshot>,
    entity_power_statuses: BTreeMap<EntityId, EntityPowerStatus>,
    fluid_networks: Vec<FluidNetworkSnapshot>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ChartState {
    pub revealed_chunks: BTreeSet<ChunkCoord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemStatistics {
    pub buckets: Vec<ItemStatisticsBucket>,
    pub last_advanced_tick: u64,
    pub rolling_produced: BTreeMap<ItemId, u64>,
    pub rolling_consumed: BTreeMap<ItemId, u64>,
    pub total_produced: BTreeMap<ItemId, u64>,
    pub total_consumed: BTreeMap<ItemId, u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemStatisticsBucket {
    pub tick: u64,
    pub produced: BTreeMap<ItemId, u64>,
    pub consumed: BTreeMap<ItemId, u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemStatisticsSnapshot {
    pub rows: Vec<ItemStatisticsRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemStatisticsRow {
    pub item_id: ItemId,
    pub produced_last_minute: u64,
    pub consumed_last_minute: u64,
    pub produced_total: u64,
    pub consumed_total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrillOutputTarget {
    InternalSlot,
    Inventory(EntityId),
    Belt(EntityId),
    Splitter {
        entity_id: EntityId,
        input_port: usize,
    },
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SimulationInput {
    BuildRedScienceResearchFixture,
}

pub type SimulationValidationError = SimValidationError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimValidationError {
    MissingTile {
        x: i32,
        y: i32,
    },
    InvalidEntityPrototype {
        entity_id: EntityId,
        prototype_id: EntityPrototypeId,
    },
    InvalidCatalogEntityPrototype {
        prototype_id: EntityPrototypeId,
    },
    InvalidEntityFootprint {
        entity_id: EntityId,
    },
    InvalidEntityTile {
        entity_id: EntityId,
        x: i32,
        y: i32,
    },
    UnknownItem(ItemId),
    InvalidFluidId(FluidId),
    EmptyItemStack(ItemId),
    StackExceedsLimit {
        item_id: ItemId,
        count: u16,
        stack_size: u16,
    },
    EntityOverlap {
        x: i32,
        y: i32,
        first: EntityId,
        second: EntityId,
    },
    OccupancyMismatch,
    OrphanEntityState(EntityId),
    InvalidEntityState {
        entity_id: EntityId,
    },
    InvalidFluidBoxState {
        entity_id: EntityId,
        box_index: usize,
    },
    InvalidFluidNetwork {
        network_id: u32,
    },
    InvalidRecipeItem {
        recipe_id: RecipeId,
        item_id: ItemId,
    },
    InvalidTechnologyItem {
        technology_id: TechnologyId,
        item_id: ItemId,
    },
    InvalidTechnologyRecipe {
        technology_id: TechnologyId,
        recipe_id: RecipeId,
    },
    InvalidTechnologyPrerequisite {
        technology_id: TechnologyId,
        prerequisite_id: TechnologyId,
    },
    InvalidCraftingRecipe {
        recipe_id: RecipeId,
    },
    InvalidBeltItemPosition {
        entity_id: EntityId,
        lane_index: usize,
        position_subtile: u16,
    },
    BeltItemSpacingViolation {
        entity_id: EntityId,
        lane_index: usize,
    },
    InvalidSplitterOutputCursor {
        entity_id: EntityId,
        lane_index: usize,
        output_port: usize,
    },
    InvalidMachineItem {
        entity_id: EntityId,
        item_id: ItemId,
    },
    InvalidMachineRecipe {
        entity_id: EntityId,
        recipe_id: RecipeId,
    },
    InvalidResearchTechnology {
        technology_id: TechnologyId,
    },
    InvalidResearchTechnologyNames,
    InvalidResearchProgress {
        technology_id: TechnologyId,
        progress_units: u32,
        required_units: u32,
    },
    InvalidActiveResearch {
        technology_id: TechnologyId,
    },
    InvalidQueuedResearch {
        technology_id: TechnologyId,
    },
    InvalidInserterTarget {
        entity_id: EntityId,
        x: i32,
        y: i32,
    },
    InvalidChartChunk(ChunkCoord),
    InvalidItemStatistics(ItemId),
}

mod belt_ops;
mod core;
mod entity_ops;
mod entity_store_ops;
mod fluid_ops;
mod generation;
mod inventory_ops;
mod machine_ops;
mod player_ops;
mod power_ops;
mod profiling;
mod research_ops;
mod save;
mod scripted;
mod statistics_ops;
mod systems;
mod validation;
mod world_ops;

use self::belt_ops::*;
use self::fluid_ops::*;
use self::generation::*;
use self::inventory_ops::*;
use self::machine_ops::*;
pub(crate) use self::profiling::{NoopTickProfiler, ProfilePhase, TickProfiler};
pub use self::profiling::{SimulationCounts, SimulationTickProfile};
pub use self::save::{
    PROTOTYPE_FORMAT_VERSION, SAVE_VERSION, SaveLoadError, load_from_bytes, prototype_hash,
    save_to_bytes,
};
pub use self::scripted::scripted_inputs_for_red_science_factory;
use self::world_ops::*;

#[cfg(test)]
mod tests;
