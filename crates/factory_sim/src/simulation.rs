use factory_data::{
    CraftingCategory, EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog, RecipeId,
    TechnologyEffect, TechnologyId, TileId,
};
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
pub const ASSEMBLING_MACHINE_INPUT_SLOT_COUNT: usize = 4;
pub const ASSEMBLING_MACHINE_OUTPUT_SLOT_COUNT: usize = 1;
pub const BELT_SUBTILES_PER_TILE: u16 = 256;
pub const BELT_ITEM_SPACING_SUBTILES: u16 = 64;
pub const BASIC_BELT_SPEED_SUBTILES_PER_TICK: u16 = 8;
pub const BASIC_INSERTER_PICKUP_TICKS: u32 = 35;
pub const BASIC_INSERTER_DROP_TICKS: u32 = 35;
const FIXED_SIM_TICKS_PER_SECOND_F64: f64 = 60.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u64);

impl EntityId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CraftingQueue {
    pub entries: VecDeque<CraftingJob>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResearchState {
    pub technology_names: Vec<String>,
    pub active: Option<TechnologyId>,
    pub technologies: Vec<TechnologyResearchState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TechnologyResearchState {
    pub technology_id: TechnologyId,
    pub progress_units: u32,
    pub unlocked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchError {
    MissingTechnology(TechnologyId),
    AlreadyResearched(TechnologyId),
    PrerequisiteLocked {
        technology_id: TechnologyId,
        prerequisite_id: TechnologyId,
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Simulation {
    tick: u64,
    world: WorldSim,
    entities: EntityStore,
    player: PlayerState,
    player_inventory: Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    pub research: ResearchState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerState {
    x: i64,
    y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManualMiningTarget {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManualMiningProgress {
    pub target: ManualMiningTarget,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub tiles: Vec<TileCell>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TileCell {
    pub tile_id: TileId,
    pub collision: TileCollision,
    pub resource: Option<ResourceCell>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileCollision {
    pub walkable: bool,
    pub buildable: bool,
    pub minable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceCell {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MinedResource {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BurnerMiningDrillState {
    pub energy: BurnerEnergy,
    pub mining_progress_ticks: u32,
    pub mining_required_ticks: u32,
    pub resource_target: Option<ManualMiningTarget>,
    pub output_slot: Option<ItemStack>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FurnaceState {
    pub input_slot: Option<ItemStack>,
    pub energy: BurnerEnergy,
    pub output_slot: Option<ItemStack>,
    pub active_recipe: Option<RecipeId>,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AssemblingMachineState {
    pub selected_recipe: Option<RecipeId>,
    pub input_inventory: Inventory,
    pub output_inventory: Inventory,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LabState {
    pub inventory: Inventory,
    pub active_technology: Option<TechnologyId>,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssemblerIngredientStatus {
    pub item: ItemId,
    pub required: u32,
    pub available: u32,
    pub missing: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BeltSegment {
    pub dir: Direction,
    pub lanes: [BeltLane; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BeltLane {
    pub items: SmallVec<[BeltItem; 8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeltItem {
    pub item_id: ItemId,
    pub position_subtile: u16,
}

#[derive(Clone, Debug)]
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
    pub fn new(dir: Direction) -> Self {
        Self {
            dir,
            lanes: [BeltLane::default(), BeltLane::default()],
        }
    }
}

impl Default for BeltSegment {
    fn default() -> Self {
        Self::new(Direction::default())
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
pub enum InserterError {
    MissingEntity(EntityId),
    NotInserter(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabError {
    MissingEntity(EntityId),
    NotLab(EntityId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityStore {
    entities: Vec<SimEntity>,
    placed_entities: BTreeMap<EntityId, PlacedEntity>,
    entity_inventories: BTreeMap<EntityId, Inventory>,
    burner_mining_drills: BTreeMap<EntityId, BurnerMiningDrillState>,
    furnaces: BTreeMap<EntityId, FurnaceState>,
    assembling_machines: BTreeMap<EntityId, AssemblingMachineState>,
    labs: BTreeMap<EntityId, LabState>,
    transport_belts: BTreeMap<EntityId, BeltSegment>,
    inserters: BTreeMap<EntityId, InserterState>,
    occupancy: OccupancyGrid,
    next_entity_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SimEntity {
    pub id: EntityId,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
    transport_belt: Option<BeltSegment>,
    inserter: Option<InserterState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    North,
    East,
    South,
    West,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityFootprint {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
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
    InvalidEntityFootprint {
        entity_id: EntityId,
    },
    InvalidEntityTile {
        entity_id: EntityId,
        x: i32,
        y: i32,
    },
    UnknownItem(ItemId),
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
    InvalidResearchProgress {
        technology_id: TechnologyId,
        progress_units: u32,
        required_units: u32,
    },
    InvalidActiveResearch {
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
mod generation;
mod inventory_ops;
mod machine_ops;
mod player_ops;
mod research_ops;
mod scripted;
mod systems;
mod validation;
mod world_ops;

use self::belt_ops::*;
use self::generation::*;
use self::inventory_ops::*;
use self::machine_ops::*;
pub use self::scripted::scripted_inputs_for_red_science_factory;
use self::world_ops::*;

#[cfg(test)]
mod tests;
