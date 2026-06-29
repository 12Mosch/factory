use factory_data::{
    CraftingCategory, EntityKind, EntityPrototypeId, FluidId, ItemId, PrototypeCatalog, RecipeId,
    TechnologyEffect, TechnologyId, TileId, UndergroundBeltPart,
};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};

pub const CHUNK_SIZE: i32 = 32;
pub const PLAYER_MOVEMENT_SPEED_TILES_PER_SECOND: f32 = 5.0;
pub const PLAYER_MINING_SPEED: f32 = 0.5;
pub const ORE_MINING_TIME_SECONDS: f32 = 1.0;
pub const MANUAL_MINING_REACH_TILES: f32 = 2.5;
pub const MANUAL_MINING_TICKS_PER_ITEM: u32 =
    (ORE_MINING_TIME_SECONDS / PLAYER_MINING_SPEED * FIXED_SIM_TICKS_PER_SECOND) as u32;
pub const PLAYER_INVENTORY_SLOT_COUNT: usize = 80;
const FIXED_SIM_TICKS_PER_SECOND: f32 = 60.0;
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

#[derive(
    Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
)]
pub struct Tick(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct EntityId(u64);

impl EntityId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub count: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum InserterState {
    WaitingForItem,
    Picking { ticks_left: u32 },
    Holding { item: ItemStack },
    Dropping { ticks_left: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryError {
    UnknownItem,
    InsufficientSpace,
    InsufficientItems,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CraftingQueue {
    pub entries: VecDeque<CraftingJob>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CraftingJob {
    pub recipe_id: RecipeId,
    pub remaining_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftingError {
    MissingRecipe(RecipeId),
    NotManualRecipe(RecipeId),
    RecipeLocked(RecipeId),
    InsufficientIngredients,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResearchState {
    pub technology_names: Vec<String>,
    pub active: Option<TechnologyId>,
    pub queue: Vec<TechnologyId>,
    pub technologies: Vec<TechnologyResearchState>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TechnologyResearchState {
    pub technology_id: TechnologyId,
    pub progress_units: u32,
    pub unlocked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchError {
    MissingTechnology(TechnologyId),
    AlreadyResearched(TechnologyId),
    AlreadyActive(TechnologyId),
    AlreadyQueued(TechnologyId),
    PrerequisiteLocked {
        technology_id: TechnologyId,
        prerequisite_id: TechnologyId,
    },
    InvalidQueueIndex {
        index: usize,
    },
    NoActiveResearch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchProgressResult {
    InProgress {
        technology_id: TechnologyId,
        progress_units: u32,
        required_units: u32,
    },
    Completed {
        technology_id: TechnologyId,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Simulation {
    tick: u64,
    world: WorldSim,
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlayerState {
    x: i64,
    y: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ManualMiningTarget {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ManualMiningProgress {
    pub target: ManualMiningTarget,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
    resource_revision: u64,
    #[serde(skip, default)]
    resource_dirty_tiles: VecDeque<ResourceTileChange>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub tiles: Vec<TileCell>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TileCell {
    pub tile_id: TileId,
    pub collision: TileCollision,
    pub resource: Option<ResourceCell>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TileCollision {
    pub walkable: bool,
    pub buildable: bool,
    pub minable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceCell {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceTileChange {
    pub revision: u64,
    pub x: i32,
    pub y: i32,
    pub resource: Option<ResourceCell>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MinedResource {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BurnerMiningDrillState {
    pub energy: BurnerEnergy,
    pub mining_progress_ticks: u32,
    pub mining_required_ticks: u32,
    pub resource_target: Option<ManualMiningTarget>,
    pub output_slot: Option<ItemStack>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FurnaceState {
    pub input_slot: Option<ItemStack>,
    pub energy: BurnerEnergy,
    pub output_slot: Option<ItemStack>,
    pub active_recipe: Option<RecipeId>,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AssemblingMachineState {
    pub selected_recipe: Option<RecipeId>,
    pub input_inventory: Inventory,
    pub output_inventory: Inventory,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LabState {
    pub inventory: Inventory,
    pub active_technology: Option<TechnologyId>,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricPoleState;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricConsumerState {
    pub work_remainder_permyriad: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SteamEngineState;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BoilerState {
    pub energy: BurnerEnergy,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OffshorePumpState;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidBoxState {
    pub fluid_id: Option<FluidId>,
    pub amount_milliunits: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PowerSummary {
    pub production_watts: u64,
    pub available_production_watts: u64,
    pub consumption_watts: u64,
    pub satisfaction_permyriad: u32,
    pub network_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PowerNetworkSnapshot {
    pub network_id: u32,
    pub pole_count: usize,
    pub producer_count: usize,
    pub consumer_count: usize,
    pub production_watts: u64,
    pub available_production_watts: u64,
    pub consumption_watts: u64,
    pub satisfaction_permyriad: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityPowerStatus {
    pub network_id: Option<u32>,
    pub satisfaction_permyriad: u32,
    pub active_usage_watts: u64,
    pub drain_watts: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidNetworkSnapshot {
    pub network_id: u32,
    pub fluid_id: Option<FluidId>,
    pub total_milliunits: u64,
    pub capacity_milliunits: u64,
    pub box_count: usize,
    pub blocked: bool,
    pub boxes: Vec<FluidNetworkBoxSnapshot>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidNetworkBoxSnapshot {
    pub entity_id: EntityId,
    pub box_index: usize,
    pub capacity_milliunits: u64,
    pub amount_milliunits: u64,
    pub fluid_id: Option<FluidId>,
    pub filter: Option<FluidId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssemblerIngredientStatus {
    pub item: ItemId,
    pub required: u32,
    pub available: u32,
    pub missing: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeltSegment {
    pub dir: Direction,
    pub speed_subtiles_per_tick: u16,
    pub underground: Option<UndergroundBeltSegment>,
    pub lanes: [BeltLane; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SplitterState {
    pub dir: Direction,
    pub speed_subtiles_per_tick: u16,
    pub input_lanes: [[BeltLane; 2]; 2],
    pub next_output_by_lane: [usize; 2],
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct UndergroundBeltSegment {
    pub part: UndergroundBeltPart,
    pub max_distance: u8,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeltLane {
    pub items: SmallVec<[BeltItem; 8]>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeltItem {
    pub item_id: ItemId,
    pub position_subtile: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BurnerEnergy {
    pub fuel_slot: Option<ItemStack>,
    pub energy_remaining_joules: f64,
    pub energy_usage_watts: f64,
}

impl PartialEq for BurnerEnergy {
    fn eq(&self, other: &Self) -> bool {
        self.fuel_slot == other.fuel_slot
            && self.energy_remaining_joules.to_bits() == other.energy_remaining_joules.to_bits()
            && self.energy_usage_watts.to_bits() == other.energy_usage_watts.to_bits()
    }
}

impl Eq for BurnerEnergy {}

impl Hash for BurnerEnergy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fuel_slot.hash(state);
        self.energy_remaining_joules.to_bits().hash(state);
        self.energy_usage_watts.to_bits().hash(state);
    }
}

impl BeltSegment {
    pub fn new(dir: Direction, speed_subtiles_per_tick: u16) -> Self {
        Self {
            dir,
            speed_subtiles_per_tick,
            underground: None,
            lanes: [BeltLane::default(), BeltLane::default()],
        }
    }

    pub fn underground(
        dir: Direction,
        speed_subtiles_per_tick: u16,
        part: UndergroundBeltPart,
        max_distance: u8,
    ) -> Self {
        Self {
            dir,
            speed_subtiles_per_tick,
            underground: Some(UndergroundBeltSegment { part, max_distance }),
            lanes: [BeltLane::default(), BeltLane::default()],
        }
    }
}

impl SplitterState {
    pub fn new(dir: Direction, speed_subtiles_per_tick: u16) -> Self {
        Self {
            dir,
            speed_subtiles_per_tick,
            input_lanes: [
                [BeltLane::default(), BeltLane::default()],
                [BeltLane::default(), BeltLane::default()],
            ],
            next_output_by_lane: [0, 0],
        }
    }
}

impl Default for BeltSegment {
    fn default() -> Self {
        Self::new(Direction::default(), 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnerDrillError {
    MissingEntity(EntityId),
    NotBurnerDrill(EntityId),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceError {
    MissingEntity(EntityId),
    NotFurnace(EntityId),
    InvalidInput(ItemId),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoilerError {
    MissingEntity(EntityId),
    NotBoiler(EntityId),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblerError {
    MissingEntity(EntityId),
    NotAssembler(EntityId),
    MissingRecipe(RecipeId),
    InvalidRecipe(RecipeId),
    RecipeLocked(RecipeId),
    RecipeChangeRequiresEmpty { entity_id: EntityId },
    InvalidInput(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltError {
    MissingEntity(EntityId),
    NotTransportBelt(EntityId),
    InvalidLane { lane_index: usize },
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitterError {
    MissingEntity(EntityId),
    NotSplitter(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InserterError {
    MissingEntity(EntityId),
    NotInserter(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabError {
    MissingEntity(EntityId),
    NotLab(EntityId),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityStore {
    entities: Vec<SimEntity>,
    placed_entities: BTreeMap<EntityId, PlacedEntity>,
    entity_inventories: BTreeMap<EntityId, Inventory>,
    burner_mining_drills: BTreeMap<EntityId, BurnerMiningDrillState>,
    furnaces: BTreeMap<EntityId, FurnaceState>,
    assembling_machines: BTreeMap<EntityId, AssemblingMachineState>,
    labs: BTreeMap<EntityId, LabState>,
    electric_poles: BTreeMap<EntityId, ElectricPoleState>,
    electric_consumers: BTreeMap<EntityId, ElectricConsumerState>,
    steam_engines: BTreeMap<EntityId, SteamEngineState>,
    boilers: BTreeMap<EntityId, BoilerState>,
    offshore_pumps: BTreeMap<EntityId, OffshorePumpState>,
    fluid_boxes: BTreeMap<EntityId, Vec<FluidBoxState>>,
    transport_belts: BTreeMap<EntityId, BeltSegment>,
    splitters: BTreeMap<EntityId, SplitterState>,
    inserters: BTreeMap<EntityId, InserterState>,
    occupancy: OccupancyGrid,
    next_entity_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SimEntity {
    pub id: EntityId,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PlacedEntity {
    pub id: EntityId,
    pub prototype_id: EntityPrototypeId,
    pub x: i32,
    pub y: i32,
    pub direction: Direction,
    pub footprint: EntityFootprint,
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

struct EntityReservation {
    prototype_id: EntityPrototypeId,
    x: i32,
    y: i32,
    direction: Direction,
    footprint: EntityFootprint,
    inventory_slot_count: Option<usize>,
    burner_mining_drill: Option<BurnerMiningDrillState>,
    furnace: Option<FurnaceState>,
    assembling_machine: Option<AssemblingMachineState>,
    lab: Option<LabState>,
    electric_pole: Option<ElectricPoleState>,
    electric_consumer: Option<ElectricConsumerState>,
    steam_engine: Option<SteamEngineState>,
    boiler: Option<BoilerState>,
    offshore_pump: Option<OffshorePumpState>,
    fluid_boxes: Option<Vec<FluidBoxState>>,
    transport_belt: Option<BeltSegment>,
    splitter: Option<SplitterState>,
    inserter: Option<InserterState>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum Direction {
    #[default]
    North,
    East,
    South,
    West,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityFootprint {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OccupancyGrid {
    // maps occupied tile -> entity id
    occupied_tiles: BTreeMap<(i32, i32), EntityId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    MissingPrototype(EntityPrototypeId),
    InvalidFootprint { width: i32, height: i32 },
    OutsideGeneratedChunks { x: i32, y: i32 },
    TileBlocked { x: i32, y: i32 },
    EntityOccupied { x: i32, y: i32, entity_id: EntityId },
    MissingEntity(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBuildError {
    Build(BuildError),
    MissingPrototype(EntityPrototypeId),
    EntityLocked {
        prototype_id: EntityPrototypeId,
    },
    MissingBuildItem {
        prototype_id: EntityPrototypeId,
    },
    ItemDoesNotBuildEntity {
        item_id: ItemId,
        prototype_id: EntityPrototypeId,
    },
    InsufficientInventory {
        item_id: ItemId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityDestroyError {
    MissingEntity(EntityId),
    MissingBuildItem { prototype_id: EntityPrototypeId },
    InsufficientInventory { item_id: ItemId },
    UnknownItem(ItemId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerError {
    MissingEntity(EntityId),
    NotContainer(EntityId),
    InvalidItem(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
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
