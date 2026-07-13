pub(crate) use factory_data::{
    CraftingCategory, EntityKind, PrototypeCatalog, ResourceExtraction, TechnologyEffect, TileId,
    UndergroundBeltPart,
};
use factory_data::{EntityPrototypeId, FluidId, ItemId, RecipeId, TechnologyId};
use serde::{Deserialize, Serialize};
pub(crate) use smallvec::SmallVec;
pub(crate) use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::hash::{Hash, Hasher};

pub use crate::combat::{
    EnemySpawnerState, GUN_TURRET_AMMO_SLOT_COUNT, GunTurretState, HealthState, RepairError,
};
pub use crate::construction::{
    Blueprint, BlueprintEntity, ConstructionError, ConstructionJob, ConstructionState, GhostEntity,
    GhostId,
};
pub use crate::crafting::{CraftingError, CraftingJob, CraftingQueue};
pub use crate::enemies::{
    Enemy, EnemyBaseId, EnemyDifficultyPreset, EnemyId, EnemyMapSnapshot, EnemyMission, EnemyMode,
    EnemyRuntimeSettings, EnemySubsystem, EnemyWorldSettings, ExpansionId, RaidId,
    SimulationConfig, ThreatEvent, ThreatEventKind, ThreatLocation, ThreatSnapshot, ThreatTier,
};
pub(crate) use crate::entities::EntityReservation;
pub(crate) use crate::entities::store::for_each_entity_state_map;
pub use crate::entities::{
    BuildError, BuildPlacementIssue, BuildPlacementIssueKind, BuildPlacementPreview, Direction,
    EntityDestroyError, EntityFootprint, EntityStore, OccupancyGrid, PlacedEntity,
    PlayerBuildError, SimEntity,
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
    AssemblerError, AssemblerIngredientStatus, AssemblingMachineState, BurnerDrillError,
    BurnerEnergy, BurnerMiningDrillState, FurnaceError, FurnaceState, LabError, LabState,
    MachineStatus, PumpjackState,
};
pub use crate::player::{ManualMiningProgress, ManualMiningTarget, PlayerState};
pub use crate::pollution::PollutionState;
pub use crate::power::{
    BoilerError, BoilerState, ElectricConsumerState, ElectricPoleState, EntityPowerStatus,
    OffshorePumpState, PowerMapConnection, PowerMapConsumer, PowerMapPole, PowerMapSnapshot,
    PowerNetworkSnapshot, PowerSummary, SteamEngineState,
};
pub use crate::research::{
    ResearchError, ResearchProgressResult, ResearchState, TechnologyResearchState,
};
pub use crate::world::{
    Chunk, ChunkCoord, MinedResource, ResourceCell, ResourceTileChange, TileCell, TileCollision,
    WorldSim, WorldTileCoord,
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
/// Fixed-point scale for free-moving positions (player, enemy units):
/// 1024 units per tile.
pub const POSITION_SCALE: i64 = 1024;
const PLAYER_POSITION_SCALE: i64 = POSITION_SCALE;
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

/// Pollution diffuses between chunks and is absorbed by terrain once per
/// interval instead of every tick.
pub const POLLUTION_SPREAD_INTERVAL_TICKS: u64 = 64;
/// Share of a chunk's pollution handed to each of its four neighbors per
/// spread interval, in permille.
pub const POLLUTION_SPREAD_PER_NEIGHBOR_PERMILLE: u64 = 20;
/// Chunks below this level keep their pollution local instead of spreading.
pub const POLLUTION_MIN_TO_SPREAD_MICRO: u64 = 100_000;
/// Residue below this level evaporates after a spread pass so the pollution
/// map stays bounded.
pub const POLLUTION_MIN_RETAINED_MICRO: u64 = 1_000;

/// Health restored per applied repair command; the app repeats the command
/// while the repair input is held.
pub const REPAIR_HEALTH_PER_ACTION: u32 = 5;
pub const REPAIR_REACH_TILES: f32 = 3.0;

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct Simulation {
    tick: u64,
    #[serde(skip, default)]
    entity_topology_revision: u64,
    #[serde(skip, default)]
    revealed_revision: u64,

    world: WorldSim,
    chart: ChartState,
    entities: EntityStore,
    construction: ConstructionState,

    player: PlayerState,
    player_inventory: Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    onboarding_progress: OnboardingProgress,
    pub research: ResearchState,

    power: PowerSubsystem,
    fluids: FluidSubsystem,
    statistics: StatisticsSubsystem,
    pollution: PollutionState,
    enemies: EnemySubsystem,
    config: SimulationConfig,

    #[serde(skip)]
    attack_targets: enemy_ops::AttackTargetCache,
    #[serde(skip)]
    transport: TransportLaneCache,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OnboardingProgress {
    pub revision: u64,
    pub iron_ore_manually_mined: u64,
    pub stone_furnaces_placed: u64,
    pub iron_plates_smelted: u64,
    pub burner_mining_drills_placed: u64,
    pub iron_ore_drill_mined: u64,
    pub transport_belts_manually_crafted: u64,
    pub electricity_generated: bool,
    pub labs_placed: u64,
    pub automation_science_packs_produced: u64,
    pub logistics_researched: bool,
    pub automation_researched: bool,
    pub assembler_items_produced: u64,
    pub logistic_science_packs_produced: u64,
    pub oil_processing_researched: bool,
    pub petroleum_gas_produced: u64,
    pub turrets_researched: bool,
    pub loaded_gun_turrets: u64,
}

impl OnboardingProgress {
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    fn add(counter: &mut u64, amount: u64) -> bool {
        let next = counter.saturating_add(amount);
        let changed = next != *counter;
        *counter = next;
        changed
    }

    fn record_counter(&mut self, select: impl FnOnce(&mut Self) -> &mut u64, amount: u64) {
        if amount != 0 && Self::add(select(self), amount) {
            self.changed();
        }
    }

    fn record_flag(&mut self, select: impl FnOnce(&mut Self) -> &mut bool) {
        let flag = select(self);
        if !*flag {
            *flag = true;
            self.changed();
        }
    }

    fn record_electricity_generated(&mut self) {
        self.record_flag(|progress| &mut progress.electricity_generated);
    }

    fn record_item_produced(
        &mut self,
        base: &factory_data::BasePrototypeIds,
        item: ItemId,
        amount: u64,
    ) {
        if item == base.items.automation_science_pack {
            self.record_counter(
                |progress| &mut progress.automation_science_packs_produced,
                amount,
            );
        } else if item == base.items.logistic_science_pack {
            self.record_counter(
                |progress| &mut progress.logistic_science_packs_produced,
                amount,
            );
        }
    }

    fn record_research_completed(&mut self, technology_name: &str) {
        match technology_name {
            "logistics" => self.record_flag(|progress| &mut progress.logistics_researched),
            "automation" => self.record_flag(|progress| &mut progress.automation_researched),
            "oil_processing" => {
                self.record_flag(|progress| &mut progress.oil_processing_researched)
            }
            "turrets" => self.record_flag(|progress| &mut progress.turrets_researched),
            _ => {}
        }
    }
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidStatistics {
    pub buckets: Vec<FluidStatisticsBucket>,
    pub last_advanced_tick: u64,
    pub rolling_produced: BTreeMap<FluidId, u64>,
    pub rolling_consumed: BTreeMap<FluidId, u64>,
    pub total_produced: BTreeMap<FluidId, u64>,
    pub total_consumed: BTreeMap<FluidId, u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidStatisticsBucket {
    pub tick: u64,
    pub produced: BTreeMap<FluidId, u64>,
    pub consumed: BTreeMap<FluidId, u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FluidStatisticsSnapshot {
    pub rows: Vec<FluidStatisticsRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FluidStatisticsRow {
    pub fluid_id: FluidId,
    pub produced_last_minute: u64,
    pub consumed_last_minute: u64,
    pub produced_total: u64,
    pub consumed_total: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PowerStatistics {
    pub samples: Vec<PowerStatisticsSample>,
    pub last_advanced_tick: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PowerStatisticsSample {
    pub tick: u64,
    pub production_watts: u64,
    pub available_production_watts: u64,
    pub consumption_watts: u64,
    pub satisfaction_permyriad: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerStatisticsSnapshot {
    pub samples: Vec<PowerStatisticsSample>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineStatusCount {
    pub status: MachineStatus,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineStatusGroup {
    pub kind: EntityKind,
    pub counts: Vec<MachineStatusCount>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineStatusSnapshot {
    pub groups: Vec<MachineStatusGroup>,
    pub total_by_status: Vec<MachineStatusCount>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BottleneckHintKind {
    ItemDeficit,
    ResearchMissingScience,
    SteamStarved,
    PowerShortage,
    NoActiveResearch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BottleneckHint {
    pub kind: BottleneckHintKind,
    pub subject_item: Option<ItemId>,
    pub subject_fluid: Option<FluidId>,
    pub affected_count: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BottleneckHintsSnapshot {
    pub hints: Vec<BottleneckHint>,
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

pub type SimulationValidationError = SimValidationError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollutionRemainderSource {
    MachineEmission(EntityId),
    TerrainAbsorption(ChunkCoord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimValidationError {
    MissingTile {
        x: WorldTileCoord,
        y: WorldTileCoord,
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
        x: WorldTileCoord,
        y: WorldTileCoord,
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
        x: WorldTileCoord,
        y: WorldTileCoord,
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
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    InvalidChartChunk(ChunkCoord),
    InvalidItemStatistics(ItemId),
    InvalidFluidStatistics(FluidId),
    InvalidPowerStatistics,
    InvalidPollutionState {
        source: PollutionRemainderSource,
    },
    InvalidGhostPrototype {
        ghost_id: GhostId,
        prototype_id: EntityPrototypeId,
    },
    InvalidGhostIdentity {
        ghost_id: GhostId,
    },
    InvalidGhostFootprint {
        ghost_id: GhostId,
    },
    InvalidGhostRecipe {
        ghost_id: GhostId,
        recipe_id: RecipeId,
    },
    GhostOccupancyMismatch,
    GhostOverlapsEntity {
        ghost_id: GhostId,
        entity_id: EntityId,
    },
    InvalidDeconstructionMark(EntityId),
    InvalidConstructionQueue,
    InvalidBlueprintPrototype {
        blueprint_index: usize,
        prototype_id: EntityPrototypeId,
    },
    InvalidBlueprintRecipe {
        blueprint_index: usize,
        recipe_id: RecipeId,
    },
    InvalidEnemy {
        enemy_id: EnemyId,
    },
    InvalidEnemyState,
}

mod belt_ops;
mod combat_ops;
mod commands;
pub mod construction_ops;
mod contexts;
mod core;
mod diagnostics_ops;
mod disjoint_set;
mod enemy_ops;
pub mod entity_access;
pub mod entity_mutation;
mod entity_recovery_ops;
mod entity_states;
mod entity_store_ops;
pub mod entity_transfer;
mod fluid_ops;
mod fluid_state;
mod generation;
mod inventory_ops;
mod machine_ops;
mod machine_tick;
pub mod placement;
mod placement_mutation_ops;
mod placement_preview_ops;
mod placement_validation_ops;
mod player_ops;
mod pollution_ops;
mod power_ops;
mod power_state;
mod profiling;
mod research_ops;
mod save;
mod scripted;
mod statistics_ops;
mod statistics_state;
mod systems;
mod topology_invalidation_ops;
mod validation;
mod world_ops;

use self::belt_ops::*;
pub use self::commands::{
    InventoryPanel, SimCommand, SimCommandEffect, SimCommandError, SlotTransferError,
};
pub use self::construction_ops::GhostPlacementRequest;
use self::contexts::*;
pub(crate) use self::entity_states::EntityStateBehavior;
use self::fluid_ops::*;
use self::fluid_state::FluidSubsystem;
use self::generation::*;
use self::inventory_ops::*;
use self::machine_ops::*;
use self::power_state::{PowerSubsystem, PowerTopologyCache};
pub(crate) use self::profiling::{NoopTickProfiler, ProfilePhase, TickProfiler};
pub use self::profiling::{SimulationCounts, SimulationTickProfile};
pub use self::save::{
    PROTOTYPE_FORMAT_VERSION, SAVE_HEADER_SIZE, SAVE_VERSION, SaveHeaderInfo, SaveLoadError,
    inspect_save_header, load_from_bytes, prototype_hash, save_to_bytes,
};
pub use self::scripted::scripted_inputs_for_red_science_factory;
use self::statistics_ops::power_sample_is_recorded;
use self::statistics_state::StatisticsSubsystem;
pub use self::world_ops::ChunkNeighborhoodError;
use self::world_ops::*;

#[cfg(test)]
mod tests;
