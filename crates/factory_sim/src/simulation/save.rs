use super::robot_ops::RobotLogisticWorkState;
use super::*;
use crate::SaveLimits;
use bincode::Options;
use std::io::{Read, Write};

// Save version 9 intentionally invalidates older saves: construction planning
// became part of deterministic simulation state and no v8 migration is kept.
// v12: pollution and enemy state (spawners, units, health, turrets) joined
// the snapshot and the entity state registry.
// v13: durable, action-specific early-game objective progress joined the snapshot.
// v14: early-game progress expanded into durable onboarding progress.
// v15: enemy settings, colonies, missions, evolution and threat events.
// v16: EnemySpawnerState dropped its unused absorbed_pollution_micro field
// (absorbed pollution is pooled on EnemyBase::attack_budget_micro).
// v17: per-source pollution emission and terrain absorption remainders joined
// the pollution snapshot.
// v18: typed combat state, factions, resistance profiles, and attack
// definitions replaced the previous untyped damage fields.
// v19: pending deterministic chunk-generation requests joined the snapshot.
// v20: furnace and mining drill energy generalized to burner-or-electric
// (MachineEnergy), enabling electric furnaces and electric mining drills.
// v21: belt items gained stable identities used by incremental presentation.
// v22: inserter energy state joined the entity registry.
// v23: laser turret and powered player equipment state joined the snapshot.
// v24: deterministic day/night cycle phase joined the snapshot.
// v25: machine module state and beacon state joined the entity registry.
// v26: solar panel and accumulator state maps and durable power storage
// statistics joined the snapshot.
// v27: radar state and durable pending radar-reveal generation requests joined
// the snapshot.
// v28: circuit wire connections, per-entity circuit configuration, combinator
// state, and lamp state joined the entity registry.
// v29: heat networks joined the snapshot, along with heat buffer, nuclear
// reactor, heat pipe, and heat exchanger state in the entity registry.
// v30: robot networks joined the snapshot, along with roboport state (robot
// slots, material slots, and the charging buffer) in the entity registry.
// v31: robots in flight joined the snapshot: their positions, energy, errands,
// and the charging pads and queues they occupy at their roboports.
// v32: construction jobs gained repair work and robot reservations; flying
// robots gained construction payload and cargo state.
// v33: logistic chest configuration (request and filter rows) joined the entity
// registry.
// v34: flying robots gained the logistic delivery they own, and robot network
// snapshots gained logistic robot and active delivery counts.
// v35: rail pieces joined the catalog. They save as ordinary placed entities,
// but the catalog they are validated against changed, and the rail graph they
// form is a derived cache rebuilt on load.
// v36: rolling stock joined the snapshot: locomotives and wagons with their
// position along a rail edge, their cargo and fuel, and the trains they are
// coupled into with a velocity mid-run.
// v37: trains gained somewhere to be: a destination on the rail graph and the
// route they are driving toward it, both durable so a train mid-plan through a
// save is still mid-plan when it loads. The rolling-stock subsystem gained the
// routing pass's cursor with them, because which trains a tick with more
// searches than budget plans for follows from it, and a train remembers a
// search that ran out of expansions so it does not repeat it every tick.
// v38: trains gained the blocks they hold. Rail and chain signals join the
// catalog as ordinary placed entities and the blocks they cut the graph into are
// a derived cache rebuilt on load, but a train's *claim* on a block is not
// derivable — which block a train was let into cannot be read off where it is
// standing — so the claims are saved with the train. Circuit entity state renamed
// its accumulator charge channel to the general output channel a rail signal
// reports its aspect on.
// v39: named train stops joined the rolling-stock subsystem, and trains gained
// the schedule that drives them between stops. None of it is derivable: which
// stop a train has claimed is a reservation against that stop's train limit, and
// how long it has been waiting — and how long since its cargo last changed —
// decides when it leaves, so a save that rebuilt any of it would depart trains at
// different moments than the world it was saved from.
// v40: wagons joined the factory. Inventories gained per-slot item filters, so
// every saved inventory carries a filter row (empty for all the ones nobody has
// filtered); fluid network box snapshots name their holder rather than an entity
// id, because a stopped fluid wagon is part of the network at the pump it stands
// at and a wagon has no entity id to be named by. A number of its own rather
// than sharing v39 with the stops above: that version already describes a
// released format, and a save written here holds both changes rather than
// either.
// v41: a train stop became a placed entity rather than a mark held by the
// rolling-stock subsystem. Its name, train limit, and the channel that limit may
// be read from join the entity state registry; the mark it puts on the track is
// derived with the rail graph and no longer saved, and a train's claim on a stop
// names that entity. Wait conditions gained a comparison against the signals
// reaching the stop, which is what the connector on it is for.
// v42: a train records whether the player is driving it. The flag is durable
// because a save that dropped it would hand every hand-parked train back to its
// schedule on load and send it off again.
// v43: rocket silo state was appended — the ingredients of the part being
// built and the count of parts already standing as a rocket. The count is the
// rocket: nothing else records that a silo is part-way through one.
// v44: rocket silos gained a cargo slot and durable fixed-tick launch phase.
// v45: rocket silos gained a launch-product output inventory.
// v46: the durable rockets-launched statistic joined the snapshot.
// v47: research completion became level-based so repeatable technology levels
// and their in-level progress are durable. Mining drills also gained durable
// pending output so unbounded productivity bonuses can drain through bounded
// inventories without truncation or stalling. Construction robots retain such
// deconstruction yields as compact bulk cargo until storage can accept them.
// v48: the powered-silo and completed-rocket-parts onboarding milestones joined
// the snapshot. Unlike the surrounding production, research, and launch totals,
// these historical transitions cannot be reconstructed after a silo loses
// power, launches, or is removed.
// v49: personal roboport buffer and charging-pad state joined powered equipment,
// and flying robots gained durable personal ownership.
// v50: manual crafting jobs gained stable identities and the queue gained
// durable identity and completion cursors so cancel/reorder commands remain
// safe across saves and presentation can distinguish completion from mutation.
// v51: selected personal weapon, opened magazine, and fire cooldown joined the
// durable player state.
// v52: delayed projectiles, combat status effects, and per-module personal
// laser cooldowns joined durable combat/equipment state.
// v53: durable player death tick, pending respawn request and death statistics.
// v54: persistent player corpses, item quantities and opened consumables.
// v55: durable navigation work, target decisions, invalidation revisions, and
// belt identities and transport execution/scheduling state. v54 cannot
// reconstruct these historical values.
// v56: enemy-spawner prototypes gained a failed free-guard spawn retry interval.
// v57: enemies gained durable long-range wall-follow direction and progress state.
// v58: unfinished train route searches gained durable A* frontiers so searches
// larger than one tick's expansion slice resume identically across a save.
pub const SAVE_VERSION: u32 = 58;
/// Oldest historical simulation format this build can migrate.
///
/// Versions before this boundary omitted deterministic state that cannot be
/// reconstructed. Keep this constant and the dispatcher below in sync whenever
/// the save format changes.
pub const OLDEST_SUPPORTED_SAVE_VERSION: u32 = 57;
const CURRENT_SNAPSHOT_LAYOUT_VERSION: u32 = 58;
const _: () = assert!(
    SAVE_VERSION == CURRENT_SNAPSHOT_LAYOUT_VERSION,
    "a new save version requires a schema, migration step, or compatibility-boundary decision"
);

/// Header-level support declared by the same explicit table used by decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveVersionSupport {
    UnsupportedOld,
    Migratable,
    Current,
    Newer,
}

pub const fn save_version_support(version: u32) -> SaveVersionSupport {
    match version {
        57 => SaveVersionSupport::Migratable,
        58 => SaveVersionSupport::Current,
        0..57 => SaveVersionSupport::UnsupportedOld,
        _ => SaveVersionSupport::Newer,
    }
}
const _: () = assert!(matches!(
    save_version_support(SAVE_VERSION),
    SaveVersionSupport::Current
));
// v8: PrototypeCatalog gained the world_generation config section.
// v9: WorldGenerationConfig gained the optional distance_scaling section.
// v10: combat prototypes (health, pollution, ammo, turrets, enemy bases).
// v11: PrototypeCatalog gained the optional enemy_gameplay config section.
// v12: EntityPrototype gained the furnace section (crafting speed for
// burner-or-electric furnaces).
// v13: pumps and underground-pipe metadata joined EntityPrototype.
// v14: typed ammo, laser turrets, armor, and powered equipment metadata.
// v15: PrototypeCatalog gained the optional day_night_cycle config section.
// v16: item module effects and entity module/beacon metadata.
// v17: entity prototypes gained solar panel and accumulator metadata.
// v18: entity prototypes gained radar scan metadata.
// v19: virtual signals, circuit connectors, and combinator metadata.
// v20: item burnt results and entity heat buffer, heat energy source, and
// nuclear reactor metadata.
// v21: entity prototypes gained roboport coverage, storage, and charging
// metadata.
// v22: robot flight profiles on item prototypes, and roboport charging pads.
// v23: robot flight profiles gained an explicit construction/logistic kind.
// v24: chest prototypes gained logistic chest metadata (network role and
// request rows), and the roboport gained a circuit connector.
// v25: entity prototypes gained rail piece geometry (sub-tile ends, headings,
// and the curve between them).
// v26: entity prototypes gained rolling stock metadata (length, weight,
// braking force, top speed, and locomotive tractive force).
// v27: rail signal and chain signal entity kinds, which partition the rail
// graph into blocks.
// v28: the rocket silo entity kind and its prototype section, and the
// `RocketBuilding` crafting category the part recipe sits in.
// v29: rocket silo prototypes gained launch payload, product, and output
// capacity metadata.
// v30: technologies gained level models, cost curves, and typed simulation
// bonus effects.
// v31: launch rewards moved from the rocket silo prototype to data-driven item
// payload metadata and gained atomic multi-product support.
// v32: powered equipment gained the personal-roboport effect metadata.
// v33: item prototypes gained personal weapons and typed ammunition categories.
// v34: cone, rocket, and flame delivery metadata plus personal-laser equipment.
// v35: enemy spawners gained a data-driven failed guard-spawn retry interval.
pub const PROTOTYPE_FORMAT_VERSION: u32 = 35;

const SAVE_MAGIC: [u8; 8] = *b"FACTSIM\0";
pub const SAVE_HEADER_SIZE: usize = 8 + 4 + 4 + 8;
/// Maximum encoded durable payload, excluding the fixed save header.
/// Bounds accepted input and prevents writing worlds the loader cannot reopen.
pub const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

/// Identifies one immutable view of a world within an application session.
///
/// `world_generation` changes whenever the application installs a different
/// [`Simulation`]. `tick` is the completed simulation tick captured from that
/// generation. The identity is orchestration metadata and is deliberately not
/// written into the portable save payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SaveSnapshotIdentity {
    pub world_generation: u64,
    pub tick: u64,
}

#[derive(Debug)]
pub enum SaveLoadError {
    TooLarge,
    Codec(Box<bincode::ErrorKind>),
    InvalidMagic { found: [u8; 8] },
    UnsupportedSaveVersion { found: u32, supported: u32 },
    UnsupportedPrototypeFormatVersion { found: u32, supported: u32 },
    PrototypeHashMismatch { stored: u64, computed: u64 },
    InvalidSimulationState(SimulationValidationError),
}

impl From<bincode::Error> for SaveLoadError {
    fn from(error: bincode::Error) -> Self {
        match *error {
            bincode::ErrorKind::SizeLimit => Self::TooLarge,
            bincode::ErrorKind::Custom(ref message)
                if message == crate::save_limits::COLLECTION_LIMIT_ERROR =>
            {
                Self::TooLarge
            }
            _ => Self::Codec(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveHeaderInfo {
    pub save_version: u32,
    pub prototype_format_version: u32,
    pub prototype_hash: u64,
}

#[derive(Clone, Copy)]
struct SaveHeader {
    magic: [u8; 8],
    save_version: u32,
    prototype_format_version: u32,
    prototype_hash: u64,
}

// Single ordered durable-state registry. Borrowed encoding and detached capture
// are generated together, so adding a field cannot silently omit one save path.
// Runtime ownership and reconstruction rules are documented in save_state.md.
macro_rules! capture_snapshot_field {
    ($ty:ty, $source:expr) => {
        <$ty>::clone(&$source)
    };
    ($ty:ty, $source:expr, $capture:ident) => {
        $source.$capture()
    };
}

macro_rules! define_snapshot {
    ($sim:ident; $($field:ident: $ty:ty => $source:expr $(; $capture:ident)?),* $(,)?) => {
        #[derive(Clone, Deserialize, Serialize)]
        pub(in crate::simulation) struct SimulationSnapshotOwned {
            $(pub(in crate::simulation) $field: $ty,)*
        }

        #[derive(Serialize)]
        struct SimulationSnapshotRef<'a> {
            $($field: &'a $ty,)*
        }

        impl<'a> SimulationSnapshotRef<'a> {
            /// Borrows the complete ordered durable-state registry for encoding.
            fn from_simulation($sim: &'a Simulation) -> Self {
                Self { $($field: &$source,)* }
            }
        }

        impl SimulationSnapshotOwned {
            /// Clones the complete ordered durable-state registry for detached encoding.
            fn from_simulation($sim: &Simulation) -> Self {
                Self { $($field: capture_snapshot_field!($ty, $source $(, $capture)?),)* }
            }
        }
    };
}

define_snapshot! {
    sim;
    tick: u64 => sim.tick,
    day_night_cycle: Option<DayNightCycleState> => sim.day_night_cycle,
    world_seed: u64 => sim.world.seed,
    prototypes: PrototypeCatalog => sim.world.prototypes,
    chunks: BTreeMap<ChunkCoord, Chunk> => sim.world.chunks,
    chunk_generation_queue: ChunkGenerationQueue => sim.chunk_generation_queue,
    chart: ChartState => sim.chart,
    item_statistics: ItemStatistics => sim.statistics.items,
    fluid_statistics: FluidStatistics => sim.statistics.fluids,
    power_statistics: PowerStatistics => sim.statistics.power,
    rockets_launched: u64 => sim.statistics.rockets_launched,
    player_deaths: u64 => sim.statistics.player_deaths,
    entities: EntityStore => sim.entities,
    construction: ConstructionState => sim.construction,
    player: PlayerState => sim.player,
    player_equipment: PlayerEquipmentState => sim.player_equipment,
    player_weapon: PlayerWeaponState => sim.player_weapon,
    delayed_combat: DelayedCombatState => sim.delayed_combat,
    player_inventory: Inventory => sim.player_inventory,
    corpses: BTreeMap<u64, PlayerCorpse> => sim.corpses,
    manual_mining_progress: Option<ManualMiningProgress> => sim.manual_mining_progress,
    crafting_queue: CraftingQueue => sim.crafting_queue,
    onboarding_progress: OnboardingProgress => sim.onboarding_progress,
    research: ResearchState => sim.research,
    power_summary: PowerSummary => sim.power.summary,
    power_networks: Vec<PowerNetworkSnapshot> => sim.power.networks,
    entity_power_statuses: DenseEntityMap<EntityPowerStatus> => sim.power.entity_statuses,
    fluid_networks: Vec<FluidNetworkSnapshot> => sim.fluids.networks,
    fluid_topology_dirty: bool => sim.fluids.topology_dirty,
    heat_networks: Vec<HeatNetworkSnapshot> => sim.heat.networks,
    heat_topology_dirty: bool => sim.heat.topology_dirty,
    robot_networks: Vec<RobotNetworkSnapshot> => sim.robots.networks,
    robot_logistic_work: RobotLogisticWorkState => sim.robots.logistic_work,
    robot_flights: RobotFlightSubsystem => sim.robot_flights,
    rolling_stock: RollingStockSubsystem => sim.rolling_stock,
    pending_train_route_searches: BTreeMap<TrainId, rolling_stock_ops::PendingTrainRouteSearch> => sim.train_routing.pending,
    pollution: PollutionState => sim.pollution,
    enemies: EnemySubsystem => sim.enemies,
    config: SimulationConfig => sim.config,
    entity_topology_revision: u64 => sim.entity_topology_revision,
    world_chunk_revision: u64 => sim.world.chunk_revision,
    world_walkability_revision: u64 => sim.world.walkability_revision,
    transport: TransportLaneCache => sim.transport; clone_for_save,
    enemy_navigation: enemy::EnemyNavigation => sim.enemy_navigation; clone_for_save,
    attack_targets: enemy::AttackTargetCache => sim.attack_targets; clone_for_save,
}

/// Frozen top-level v57 wire layout.
///
/// Nested types are intentionally the runtime types that v57 used. The checked-in
/// fixture guards their continued compatibility; changing one requires either a
/// historical adapter or an explicit compatibility-boundary decision.
#[derive(Deserialize, Serialize)]
struct SimulationSnapshotV57 {
    tick: u64,
    day_night_cycle: Option<DayNightCycleState>,
    world_seed: u64,
    prototypes: PrototypeCatalog,
    chunks: BTreeMap<ChunkCoord, Chunk>,
    chunk_generation_queue: ChunkGenerationQueue,
    chart: ChartState,
    item_statistics: ItemStatistics,
    fluid_statistics: FluidStatistics,
    power_statistics: PowerStatistics,
    rockets_launched: u64,
    player_deaths: u64,
    entities: EntityStore,
    construction: ConstructionState,
    player: PlayerState,
    player_equipment: PlayerEquipmentState,
    player_weapon: PlayerWeaponState,
    delayed_combat: DelayedCombatState,
    player_inventory: Inventory,
    corpses: BTreeMap<u64, PlayerCorpse>,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    onboarding_progress: OnboardingProgress,
    research: ResearchState,
    power_summary: PowerSummary,
    power_networks: Vec<PowerNetworkSnapshot>,
    entity_power_statuses: DenseEntityMap<EntityPowerStatus>,
    fluid_networks: Vec<FluidNetworkSnapshot>,
    fluid_topology_dirty: bool,
    heat_networks: Vec<HeatNetworkSnapshot>,
    heat_topology_dirty: bool,
    robot_networks: Vec<RobotNetworkSnapshot>,
    robot_logistic_work: RobotLogisticWorkState,
    robot_flights: RobotFlightSubsystem,
    rolling_stock: RollingStockSubsystem,
    pollution: PollutionState,
    enemies: EnemySubsystem,
    config: SimulationConfig,
    entity_topology_revision: u64,
    world_chunk_revision: u64,
    world_walkability_revision: u64,
    transport: TransportLaneCache,
    enemy_navigation: enemy::EnemyNavigation,
    attack_targets: enemy::AttackTargetCache,
}

impl From<SimulationSnapshotV57> for SimulationSnapshotOwned {
    fn from(snapshot: SimulationSnapshotV57) -> Self {
        Self {
            tick: snapshot.tick,
            day_night_cycle: snapshot.day_night_cycle,
            world_seed: snapshot.world_seed,
            prototypes: snapshot.prototypes,
            chunks: snapshot.chunks,
            chunk_generation_queue: snapshot.chunk_generation_queue,
            chart: snapshot.chart,
            item_statistics: snapshot.item_statistics,
            fluid_statistics: snapshot.fluid_statistics,
            power_statistics: snapshot.power_statistics,
            rockets_launched: snapshot.rockets_launched,
            player_deaths: snapshot.player_deaths,
            entities: snapshot.entities,
            construction: snapshot.construction,
            player: snapshot.player,
            player_equipment: snapshot.player_equipment,
            player_weapon: snapshot.player_weapon,
            delayed_combat: snapshot.delayed_combat,
            player_inventory: snapshot.player_inventory,
            corpses: snapshot.corpses,
            manual_mining_progress: snapshot.manual_mining_progress,
            crafting_queue: snapshot.crafting_queue,
            onboarding_progress: snapshot.onboarding_progress,
            research: snapshot.research,
            power_summary: snapshot.power_summary,
            power_networks: snapshot.power_networks,
            entity_power_statuses: snapshot.entity_power_statuses,
            fluid_networks: snapshot.fluid_networks,
            fluid_topology_dirty: snapshot.fluid_topology_dirty,
            heat_networks: snapshot.heat_networks,
            heat_topology_dirty: snapshot.heat_topology_dirty,
            robot_networks: snapshot.robot_networks,
            robot_logistic_work: snapshot.robot_logistic_work,
            robot_flights: snapshot.robot_flights,
            rolling_stock: snapshot.rolling_stock,
            pending_train_route_searches: BTreeMap::new(),
            pollution: snapshot.pollution,
            enemies: snapshot.enemies,
            config: snapshot.config,
            entity_topology_revision: snapshot.entity_topology_revision,
            world_chunk_revision: snapshot.world_chunk_revision,
            world_walkability_revision: snapshot.world_walkability_revision,
            transport: snapshot.transport,
            enemy_navigation: snapshot.enemy_navigation,
            attack_targets: snapshot.attack_targets,
        }
    }
}

#[cfg(test)]
impl SimulationSnapshotV57 {
    fn from_simulation(sim: &Simulation) -> Self {
        Self {
            tick: sim.tick,
            day_night_cycle: sim.day_night_cycle,
            world_seed: sim.world.seed,
            prototypes: sim.world.prototypes.clone(),
            chunks: sim.world.chunks.clone(),
            chunk_generation_queue: sim.chunk_generation_queue.clone(),
            chart: sim.chart.clone(),
            item_statistics: sim.statistics.items.clone(),
            fluid_statistics: sim.statistics.fluids.clone(),
            power_statistics: sim.statistics.power.clone(),
            rockets_launched: sim.statistics.rockets_launched,
            player_deaths: sim.statistics.player_deaths,
            entities: sim.entities.clone(),
            construction: sim.construction.clone(),
            player: sim.player,
            player_equipment: sim.player_equipment.clone(),
            player_weapon: sim.player_weapon,
            delayed_combat: sim.delayed_combat.clone(),
            player_inventory: sim.player_inventory.clone(),
            corpses: sim.corpses.clone(),
            manual_mining_progress: sim.manual_mining_progress,
            crafting_queue: sim.crafting_queue.clone(),
            onboarding_progress: sim.onboarding_progress,
            research: sim.research.clone(),
            power_summary: sim.power.summary,
            power_networks: sim.power.networks.clone(),
            entity_power_statuses: sim.power.entity_statuses.clone(),
            fluid_networks: sim.fluids.networks.clone(),
            fluid_topology_dirty: sim.fluids.topology_dirty,
            heat_networks: sim.heat.networks.clone(),
            heat_topology_dirty: sim.heat.topology_dirty,
            robot_networks: sim.robots.networks.clone(),
            robot_logistic_work: sim.robots.logistic_work.clone(),
            robot_flights: sim.robot_flights.clone(),
            rolling_stock: sim.rolling_stock.clone(),
            pollution: sim.pollution.clone(),
            enemies: sim.enemies.clone(),
            config: sim.config,
            entity_topology_revision: sim.entity_topology_revision,
            world_chunk_revision: sim.world.chunk_revision,
            world_walkability_revision: sim.world.walkability_revision,
            transport: sim.transport.clone_for_save(),
            enemy_navigation: sim.enemy_navigation.clone_for_save(),
            attack_targets: sim.attack_targets.clone_for_save(),
        }
    }
}

/// An owned, immutable copy of the durable state for one completed simulation tick.
///
/// Capturing the snapshot performs the state copies needed to release the live
/// simulation immediately. Encoding can then happen on another thread without
/// borrowing or locking the simulation.
pub struct SimulationSaveSnapshot {
    identity: SaveSnapshotIdentity,
    prototype_hash: u64,
    state: SimulationSnapshotOwned,
}

impl SimulationSaveSnapshot {
    /// Returns the borrowed durable state for record-container encoding.
    pub(in crate::simulation) fn snapshot_state(&self) -> &SimulationSnapshotOwned {
        &self.state
    }

    /// Returns the completed simulation tick represented by this snapshot.
    pub fn tick_count(&self) -> u64 {
        self.identity.tick
    }

    /// Returns the world generation and completed tick captured by this handle.
    pub fn identity(&self) -> SaveSnapshotIdentity {
        self.identity
    }

    /// Returns the world seed preserved by this snapshot.
    pub fn world_seed(&self) -> u64 {
        self.state.world_seed
    }
}

/// Captures the durable state at the simulation's current completed-tick boundary.
///
/// This compatibility entry point does not attach an application world
/// generation and does not perform a size preflight. Background save
/// orchestration should use [`try_capture_save_snapshot`] instead.
pub fn capture_save_snapshot(sim: &Simulation) -> SimulationSaveSnapshot {
    capture_save_snapshot_in_generation(sim, 0)
}

pub(in crate::simulation) fn capture_save_snapshot_in_generation(
    sim: &Simulation,
    world_generation: u64,
) -> SimulationSaveSnapshot {
    SimulationSaveSnapshot {
        identity: SaveSnapshotIdentity {
            world_generation,
            tick: sim.tick,
        },
        prototype_hash: prototype_hash(&sim.world.prototypes),
        state: SimulationSnapshotOwned::from_simulation(sim),
    }
}

/// Checks the default save limits before allocating an owned snapshot, then
/// captures one immutable completed-tick generation.
///
/// The preflight walks the borrowed durable schema and computes its wire size,
/// so an unsupported world fails without first cloning the whole world. The
/// live simulation must remain read-locked by the caller for this function's
/// duration; no state from different ticks can enter the resulting handle.
pub fn try_capture_save_snapshot(
    sim: &Simulation,
    world_generation: u64,
) -> Result<SimulationSaveSnapshot, SaveLoadError> {
    try_capture_save_snapshot_with_limits(sim, world_generation, SaveLimits::default())
}

pub fn try_capture_save_snapshot_with_limits(
    sim: &Simulation,
    world_generation: u64,
    limits: SaveLimits,
) -> Result<SimulationSaveSnapshot, SaveLoadError> {
    let snapshot = SimulationSnapshotRef::from_simulation(sim);
    preflight_snapshot_with_limits(&snapshot, limits)?;
    Ok(capture_save_snapshot_in_generation(sim, world_generation))
}

/// Serializes a previously captured snapshot without accessing the live simulation.
pub fn save_snapshot_to_bytes(snapshot: &SimulationSaveSnapshot) -> Result<Vec<u8>, SaveLoadError> {
    save_snapshot_to_bytes_with_limits(snapshot, SaveLimits::default())
}

pub fn save_snapshot_to_bytes_with_limits(
    snapshot: &SimulationSaveSnapshot,
    limits: SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    let mut bytes = Vec::with_capacity(SAVE_HEADER_SIZE);
    save_snapshot_to_writer_with_limits(snapshot, &mut bytes, limits)?;
    Ok(bytes)
}

/// Serializes a captured immutable snapshot directly into `writer`.
pub fn save_snapshot_to_writer(
    snapshot: &SimulationSaveSnapshot,
    writer: &mut impl Write,
) -> Result<(), SaveLoadError> {
    save_snapshot_to_writer_with_limits(snapshot, writer, SaveLimits::default())
}

pub fn save_snapshot_to_writer_with_limits(
    snapshot: &SimulationSaveSnapshot,
    writer: &mut impl Write,
    limits: SaveLimits,
) -> Result<(), SaveLoadError> {
    encode_snapshot_into_with_limits(snapshot.prototype_hash, &snapshot.state, writer, limits)
}

pub fn save_to_bytes(sim: &Simulation) -> Result<Vec<u8>, SaveLoadError> {
    save_to_bytes_with_limits(sim, SaveLimits::default())
}

pub fn save_to_bytes_with_limits(
    sim: &Simulation,
    limits: SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    let mut bytes = Vec::with_capacity(SAVE_HEADER_SIZE);
    save_to_writer_with_limits(sim, &mut bytes, limits)?;
    Ok(bytes)
}

/// Serializes the live simulation directly into `writer`.
pub fn save_to_writer(sim: &Simulation, writer: &mut impl Write) -> Result<(), SaveLoadError> {
    save_to_writer_with_limits(sim, writer, SaveLimits::default())
}

pub fn save_to_writer_with_limits(
    sim: &Simulation,
    writer: &mut impl Write,
    limits: SaveLimits,
) -> Result<(), SaveLoadError> {
    let prototype_hash = prototype_hash(&sim.world.prototypes);
    let snapshot = SimulationSnapshotRef::from_simulation(sim);
    encode_snapshot_into_with_limits(prototype_hash, &snapshot, writer, limits)
}

fn encode_snapshot_into_with_limits(
    prototype_hash: u64,
    snapshot: &impl Serialize,
    writer: &mut impl Write,
    limits: SaveLimits,
) -> Result<(), SaveLoadError> {
    encode_versioned_snapshot_into_with_limits(
        SAVE_VERSION,
        prototype_hash,
        snapshot,
        writer,
        limits,
    )
}

fn encode_versioned_snapshot_into_with_limits(
    save_version: u32,
    prototype_hash: u64,
    snapshot: &impl Serialize,
    writer: &mut impl Write,
    limits: SaveLimits,
) -> Result<(), SaveLoadError> {
    if limits.max_encoded_bytes < SAVE_HEADER_SIZE as u64 {
        return Err(SaveLoadError::TooLarge);
    }
    crate::save_limits::check_collections(snapshot, limits)?;
    // Bincode performs its bounded size pass before emitting payload bytes.
    // Write the header only after that pass succeeds, so a size failure cannot
    // leave a writer holding a plausible partial save.
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(limits.payload_bytes())
        .serialized_size(snapshot)
        .map_err(SaveLoadError::from)?;
    writer.write_all(&SAVE_MAGIC).map_err(io_save_error)?;
    writer
        .write_all(&save_version.to_le_bytes())
        .map_err(io_save_error)?;
    writer
        .write_all(&PROTOTYPE_FORMAT_VERSION.to_le_bytes())
        .map_err(io_save_error)?;
    writer
        .write_all(&prototype_hash.to_le_bytes())
        .map_err(io_save_error)?;
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize_into(writer, snapshot)
        .map_err(SaveLoadError::from)?;
    Ok(())
}

#[cfg(test)]
fn encode_snapshot_with_limits(
    prototype_hash: u64,
    snapshot: &impl Serialize,
    limits: SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    let mut bytes = Vec::with_capacity(SAVE_HEADER_SIZE);
    encode_snapshot_into_with_limits(prototype_hash, snapshot, &mut bytes, limits)?;
    Ok(bytes)
}

/// Walks the borrowed durable schema for collection-count violations without
/// cloning. The indexed record container uses this as its capture preflight
/// so aggregate byte budgets stay record-aware instead of running the
/// monolithic whole-snapshot size pass.
pub(in crate::simulation) fn check_borrowed_snapshot_collections(
    sim: &Simulation,
    limits: SaveLimits,
) -> Result<(), SaveLoadError> {
    let snapshot = SimulationSnapshotRef::from_simulation(sim);
    crate::save_limits::check_collections(&snapshot, limits).map_err(SaveLoadError::from)
}

fn preflight_snapshot_with_limits(
    snapshot: &impl Serialize,
    limits: SaveLimits,
) -> Result<(), SaveLoadError> {
    if limits.max_encoded_bytes < SAVE_HEADER_SIZE as u64 {
        return Err(SaveLoadError::TooLarge);
    }
    crate::save_limits::check_collections(snapshot, limits)?;
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(limits.payload_bytes())
        .serialized_size(snapshot)
        .map(|_| ())
        .map_err(SaveLoadError::from)
}

pub fn load_from_bytes(bytes: &[u8]) -> Result<Simulation, SaveLoadError> {
    load_from_bytes_with_limits(bytes, SaveLimits::default())
}

pub fn load_from_bytes_with_limits(
    bytes: &[u8],
    limits: SaveLimits,
) -> Result<Simulation, SaveLoadError> {
    // The complete artifact is bounded by the encoded budget. Each encoding
    // enforces its own aggregate decoded bound while streaming, and
    // `max_record_bytes` stays per-record so partitioned worlds larger than
    // one record remain loadable.
    if bytes.len() as u64 > limits.max_encoded_bytes {
        return Err(SaveLoadError::TooLarge);
    }
    load_from_reader_with_limits(&mut std::io::Cursor::new(bytes), limits)
}

/// Decodes and validates one complete save from `reader` without buffering the
/// encoded payload. The candidate world is returned only after all durable and
/// rebuilt-state validation succeeds.
pub fn load_from_reader(reader: &mut impl Read) -> Result<Simulation, SaveLoadError> {
    load_from_reader_with_limits(reader, SaveLimits::default())
}

pub fn load_from_reader_with_limits(
    reader: &mut impl Read,
    limits: SaveLimits,
) -> Result<Simulation, SaveLoadError> {
    if limits.max_encoded_bytes < SAVE_HEADER_SIZE as u64 {
        return Err(SaveLoadError::TooLarge);
    }
    let mut prefix = [0; SAVE_HEADER_SIZE];
    reader.read_exact(&mut prefix).map_err(io_save_error)?;
    // The record container shares the first 24 header bytes (magic, save
    // version, prototype format version, prototype hash) so version
    // classification works identically for both encodings.
    if prefix[..8] == super::save_records::RECORD_MAGIC {
        return super::save_records::load_after_prefix(&prefix, reader, limits);
    }
    let (header, _) = read_header(&prefix)?;
    validate_header(header)?;

    let (snapshot, payload_bytes) =
        decode_and_migrate_snapshot(header.save_version, reader, limits)?;
    reject_trailing_or_oversized(reader, payload_bytes, limits.payload_bytes())?;

    finish_load(header, snapshot)
}

/// Dispatches each immutable historical wire schema through one explicit step.
fn decode_and_migrate_snapshot(
    version: u32,
    reader: &mut impl Read,
    limits: SaveLimits,
) -> Result<(SimulationSnapshotOwned, u64), SaveLoadError> {
    match save_version_support(version) {
        SaveVersionSupport::Migratable => match version {
            57 => {
                let (snapshot, bytes): (SimulationSnapshotV57, u64) =
                    crate::save_limits::deserialize_from(reader, limits)
                        .map_err(SaveLoadError::from)?;
                Ok((migrate_v57_to_v58(snapshot), bytes))
            }
            _ => unreachable!("the support table lists every migratable schema"),
        },
        SaveVersionSupport::Current => {
            crate::save_limits::deserialize_from(reader, limits).map_err(SaveLoadError::from)
        }
        SaveVersionSupport::UnsupportedOld | SaveVersionSupport::Newer => {
            Err(SaveLoadError::UnsupportedSaveVersion {
                found: version,
                supported: SAVE_VERSION,
            })
        }
    }
}

fn migrate_v57_to_v58(snapshot: SimulationSnapshotV57) -> SimulationSnapshotOwned {
    // v57 had no resumable train-search frontier. An empty pending set is the
    // only truthful default; all state that existed in v57 is moved unchanged.
    snapshot.into()
}

fn reject_trailing_or_oversized(
    reader: &mut impl Read,
    payload_bytes: u64,
    maximum: u64,
) -> Result<(), SaveLoadError> {
    let mut total = payload_bytes;
    let mut found_trailing = false;
    let mut trailing = [0; 8192];
    loop {
        let remaining = maximum.saturating_sub(total);
        let count = usize::try_from(remaining.saturating_add(1).min(trailing.len() as u64))
            .expect("trailing read length is bounded by the buffer length");
        match reader.read(&mut trailing[..count]) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_save_error(error)),
            Ok(0) => break,
            Ok(read) => {
                found_trailing = true;
                total += read as u64;
                if total > maximum {
                    return Err(SaveLoadError::TooLarge);
                }
            }
        }
    }
    if found_trailing {
        Err(SaveLoadError::Codec(
            bincode::ErrorKind::Custom("save payload has trailing bytes".into()).into(),
        ))
    } else {
        Ok(())
    }
}

impl SaveLoadError {
    /// Extracts an underlying reader/writer failure while keeping an
    /// unexpected EOF classified as malformed or truncated save data.
    pub fn into_io_error(self) -> Result<std::io::Error, Self> {
        match self {
            Self::Codec(codec) => match *codec {
                bincode::ErrorKind::Io(error)
                    if error.kind() != std::io::ErrorKind::UnexpectedEof =>
                {
                    Ok(error)
                }
                kind => Err(Self::Codec(Box::new(kind))),
            },
            error => Err(error),
        }
    }
}

fn finish_load(
    header: SaveHeader,
    snapshot: SimulationSnapshotOwned,
) -> Result<Simulation, SaveLoadError> {
    let computed_hash = prototype_hash(&snapshot.prototypes);
    if header.prototype_hash != computed_hash {
        return Err(SaveLoadError::PrototypeHashMismatch {
            stored: header.prototype_hash,
            computed: computed_hash,
        });
    }
    let sim = snapshot.into_simulation()?;
    sim.validate_state()
        .map_err(SaveLoadError::InvalidSimulationState)?;
    Ok(sim)
}

fn validate_header(header: SaveHeader) -> Result<(), SaveLoadError> {
    if header.magic != SAVE_MAGIC {
        return Err(SaveLoadError::InvalidMagic {
            found: header.magic,
        });
    }
    if matches!(
        save_version_support(header.save_version),
        SaveVersionSupport::UnsupportedOld | SaveVersionSupport::Newer
    ) {
        return Err(SaveLoadError::UnsupportedSaveVersion {
            found: header.save_version,
            supported: SAVE_VERSION,
        });
    }
    if header.prototype_format_version != PROTOTYPE_FORMAT_VERSION {
        return Err(SaveLoadError::UnsupportedPrototypeFormatVersion {
            found: header.prototype_format_version,
            supported: PROTOTYPE_FORMAT_VERSION,
        });
    }
    Ok(())
}

fn io_save_error(error: std::io::Error) -> SaveLoadError {
    SaveLoadError::Codec(bincode::ErrorKind::Io(error).into())
}

fn read_header(bytes: &[u8]) -> Result<(SaveHeader, &[u8]), SaveLoadError> {
    if bytes.len() < SAVE_HEADER_SIZE {
        return Err(unexpected_eof_error("save header is truncated"));
    }

    let mut magic = [0; 8];
    magic.copy_from_slice(&bytes[0..8]);

    let header = SaveHeader {
        magic,
        save_version: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        prototype_format_version: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        prototype_hash: u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]),
    };

    Ok((header, &bytes[SAVE_HEADER_SIZE..]))
}

/// Inspects only the fixed simulation header. Version mismatches are returned
/// to the caller so catalogs can explain compatibility without deserializing.
///
/// The monolithic snapshot and the indexed record container share the first
/// 24 header bytes, so both encodings classify identically here.
pub fn inspect_save_header(bytes: &[u8]) -> Result<SaveHeaderInfo, SaveLoadError> {
    let (header, _) = read_header(bytes)?;
    if header.magic != SAVE_MAGIC && header.magic != super::save_records::RECORD_MAGIC {
        return Err(SaveLoadError::InvalidMagic {
            found: header.magic,
        });
    }
    Ok(SaveHeaderInfo {
        save_version: header.save_version,
        prototype_format_version: header.prototype_format_version,
        prototype_hash: header.prototype_hash,
    })
}

fn unexpected_eof_error(message: &'static str) -> SaveLoadError {
    SaveLoadError::Codec(
        bincode::ErrorKind::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            message,
        ))
        .into(),
    )
}

pub fn prototype_hash(catalog: &PrototypeCatalog) -> u64 {
    let mut hasher = StableHasher::default();
    "factory-prototype-catalog-v1".hash(&mut hasher);
    catalog.hash(&mut hasher);
    hasher.finish()
}

impl SimulationSnapshotOwned {
    /// Validates durable input, reconstructs derived indexes, and returns a live world.
    pub(in crate::simulation) fn into_simulation(self) -> Result<Simulation, SaveLoadError> {
        // Validate the catalog and chunk shape before even constructing the
        // world generator or deriving terrain absorption from saved tile ids.
        validation::validate_catalog(&self.prototypes)
            .and_then(|()| {
                validation::world::validate_snapshot_world(&self.prototypes, &self.chunks)
            })
            .map_err(SaveLoadError::InvalidSimulationState)?;
        let robot_logistic_work = self.robot_logistic_work;
        let fluid_topology_dirty = self.fluid_topology_dirty;
        let heat_topology_dirty = self.heat_topology_dirty;
        let mut sim = Simulation {
            tick: self.tick,
            day_night_cycle: self.day_night_cycle,
            entity_topology_revision: self.entity_topology_revision,
            entity_visual_changes: Default::default(),
            entity_style_revision: 0,
            entity_style_changes: Default::default(),
            revealed_revision: 0,
            revealed_chunk_history: Default::default(),
            pollution_map_revision: 0,
            enemy_map_revision: 0,
            power_map_revision: 0,
            production_status_revision: 0,
            research_revisions: ResearchRevisions::default(),
            enemy_settings_revision: 0,
            crafting_revision: 0,
            production_map_statuses: Vec::new(),
            production_map_status_scratch: Vec::new(),
            world: WorldSim::from_snapshot(self.world_seed, self.prototypes, self.chunks),
            chunk_generation_queue: self.chunk_generation_queue,
            chart: self.chart,
            entities: self.entities,
            construction: self.construction,
            player: self.player,
            respawn_search: Default::default(),
            player_equipment: self.player_equipment,
            player_weapon: self.player_weapon,
            delayed_combat: self.delayed_combat,
            player_inventory: self.player_inventory,
            corpses: self.corpses,
            manual_mining_progress: self.manual_mining_progress,
            crafting_queue: self.crafting_queue,
            onboarding_progress: self.onboarding_progress,
            research: self.research,
            power: PowerSubsystem {
                summary: self.power_summary,
                networks: self.power_networks,
                entity_statuses: self.entity_power_statuses,
                topology_dirty: true,
                topology: PowerTopologyCache::default(),
                #[cfg(test)]
                topology_rebuilds: 0,
            },
            power_demand_cache: PowerDemandCache::default(),
            power_tick_scratch: power_ops::PowerTickScratch::default(),
            fluids: FluidSubsystem::from_networks(self.fluid_networks),
            heat: HeatSubsystem::from_networks(self.heat_networks),
            rails: RailSubsystem::default(),
            train_routing: rolling_stock_ops::TrainRouting::from_pending(
                self.pending_train_route_searches,
            ),
            stopped_stock_index: rolling_stock_ops::StoppedStockIndex::default(),
            robots: RobotSubsystem::from_networks(self.robot_networks),
            robot_flights: self.robot_flights,
            rolling_stock: self.rolling_stock,
            rolling_stock_topology_revision: 0,
            circuits: CircuitSubsystem::default(),
            statistics: StatisticsSubsystem {
                items: self.item_statistics,
                fluids: self.fluid_statistics,
                power: self.power_statistics,
                rockets_launched: self.rockets_launched,
                player_deaths: self.player_deaths,
            },
            pollution: self.pollution,
            capacity_overflows: CapacityOverflowCounters::default(),
            pollution_emitters: PollutionEmitterIndex::default(),
            pollution_diffusion: PollutionDiffusionBuffer::default(),
            enemies: self.enemies,
            config: self.config,
            attack_targets: self.attack_targets,
            enemy_target_chunks: combat_ops::EnemyChunkIndex::default(),
            dynamic_unit_chunks: Default::default(),
            enemy_spawning_scratch: enemy::EnemySpawningScratch::default(),
            enemy_navigation: self.enemy_navigation,
            transport: self.transport,
        };
        sim.world.chunk_revision = self.world_chunk_revision;
        sim.world.walkability_revision = self.world_walkability_revision;
        sim.entities.rebuild_pump_registry(&sim.world.prototypes);
        validation::validate_durable_state(&sim).map_err(SaveLoadError::InvalidSimulationState)?;
        // The rail graph is a derived cache like the circuit topology, so a
        // loaded world rebuilds it before anything can ask what connects — and
        // before the stopped-stock index, which is read off the geometry it
        // holds.
        // Arrival-index reconstruction may invalidate the fluid cache. Keep
        // the encoded summaries intact until they have been validated.
        let saved_fluid_networks = std::mem::take(&mut sim.fluids.networks);
        sim.ensure_rail_graph();
        // Ahead of the fluid topology, because a stopped fluid wagon is part of
        // the network at the pump it is standing at: a topology built before the
        // index would leave the wagon out, and the saved network snapshots —
        // taken from a world that had it in — would not describe it.
        sim.refresh_stopped_stock_index();
        sim.fluids.networks = saved_fluid_networks;
        sim.fluids.topology_dirty = fluid_topology_dirty;
        sim.heat.topology_dirty = heat_topology_dirty;
        // Check saved summaries before reconstruction can replace them.
        validation::validate_derived_state(&sim).map_err(SaveLoadError::InvalidSimulationState)?;
        // Clean summaries need their private topology indexes rebuilt before
        // use. A pending invalidation must remain pending, however: rebuilding
        // it here would publish summaries and consume work before the next tick.
        if !fluid_topology_dirty {
            sim.fluids.topology_dirty = true;
            sim.ensure_fluid_network_topology();
            // The snapshots are re-derived rather than trusted, because the
            // index above may have joined a wagon onto a network and cleared
            // them. Validation guarantees the result matches the saved view.
            sim.refresh_fluid_network_snapshots();
        }
        if !heat_topology_dirty {
            sim.heat.topology_dirty = true;
            sim.ensure_heat_network_topology();
            sim.refresh_heat_network_snapshots();
        }
        // Robot coverage queries read the topology cache, so rebuild it before
        // anything can ask a loaded world which network covers a tile.
        sim.ensure_robot_network_topology();
        sim.refresh_logistic_index();
        let robot_network_count = sim.robots.topology_networks.len();
        if !sim
            .robots
            .logistic_work
            .restore(robot_logistic_work, robot_network_count)
        {
            return Err(SaveLoadError::InvalidSimulationState(
                SimValidationError::InvalidRobotNetwork {
                    network_id: u32::try_from(robot_network_count).unwrap_or(u32::MAX),
                },
            ));
        }
        sim.rebuild_circuit_state();
        sim.rebuild_all_module_effects();
        sim.rebuild_pollution_emitter_index();
        sim.attack_targets.rebuild_index(&sim.world, &sim.entities);
        sim.enemy_target_chunks.rebuild(&sim.enemies);
        sim.refresh_dynamic_unit_chunk_index();
        Ok(sim)
    }
}

/// Shared headless continuation runner: commands at a relative tick execute in
/// slice order, before that tick, on both the uninterrupted and restored world.
#[cfg(test)]
pub(in crate::simulation) fn assert_save_continuation(
    original: &mut Simulation,
    ticks: usize,
    commands: &[(usize, SimCommand)],
) {
    let bytes = save_to_bytes(original).unwrap();
    let detached = capture_save_snapshot(original);
    assert_eq!(bytes, save_snapshot_to_bytes(&detached).unwrap());
    let mut restored = load_from_bytes(&bytes).unwrap();
    assert_eq!(original.state_hash(), restored.state_hash(), "at load");
    for tick in 0..ticks {
        for (_, command) in commands.iter().filter(|(at, _)| *at == tick) {
            assert_eq!(
                original.apply_command(command),
                restored.apply_command(command),
                "command at relative tick {tick}: {command:?}"
            );
        }
        original.tick();
        restored.tick();
        assert_eq!(
            original.state_hash(),
            restored.state_hash(),
            "first continuation divergence at tick {} (relative {tick})",
            original.tick
        );
        original.validate_state().unwrap();
        restored.validate_state().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::io;

    struct FragmentedReader {
        bytes: io::Cursor<Vec<u8>>,
        interrupt_next: bool,
    }

    impl Read for FragmentedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.interrupt_next {
                self.interrupt_next = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.interrupt_next = true;
            let count = buffer.len().min(3);
            self.bytes.read(&mut buffer[..count])
        }
    }

    #[derive(Default)]
    struct FragmentedWriter {
        bytes: Vec<u8>,
        interrupt_next: bool,
    }

    impl Write for FragmentedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.interrupt_next {
                self.interrupt_next = false;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.interrupt_next = true;
            let count = buffer.len().min(3);
            self.bytes.extend_from_slice(&buffer[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingIo;

    fn sanitized_v57_fixture_simulation() -> Simulation {
        let (mut simulation, rails) =
            crate::simulation::tests::rolling_stock::world_with_rail_run(24);
        for _ in 1..64 {
            simulation.tick();
        }
        let stock_id = crate::simulation::tests::rolling_stock::place_stock(
            &mut simulation,
            &rails,
            8,
            "locomotive",
        )
        .expect("the sanitized locomotive fits on its rail run");
        let train_id = simulation
            .rolling_stock_piece(stock_id)
            .expect("the sanitized locomotive was placed")
            .train;
        simulation
            .set_train_destination(train_id, rails[20])
            .expect("the sanitized train accepts its destination");
        let standing = simulation
            .rolling_stock_piece(stock_id)
            .expect("the sanitized locomotive remains placed")
            .position;
        let train = simulation
            .rolling_stock
            .trains
            .get_mut(&train_id)
            .expect("the sanitized train exists");
        train.route = None;
        train.route_search_exhausted_at = Some(standing);
        simulation
            .validate_state()
            .expect("the historical exhausted-search marker is valid without a frontier");
        simulation
    }

    #[test]
    #[ignore = "fixture regeneration is an explicit maintainer action"]
    fn regenerate_sanitized_v57_fixture() {
        let simulation = sanitized_v57_fixture_simulation();
        let snapshot = SimulationSnapshotV57::from_simulation(&simulation);
        let mut bytes = Vec::new();
        encode_versioned_snapshot_into_with_limits(
            57,
            prototype_hash(&simulation.world.prototypes),
            &snapshot,
            &mut bytes,
            SaveLimits::default(),
        )
        .unwrap();
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("save-v57-sanitized.factsim"), bytes).unwrap();
    }

    #[test]
    fn v57_fixture_migrates_validates_and_continues_deterministically() {
        let bytes = include_bytes!("../../tests/fixtures/save-v57-sanitized.factsim");
        let header = inspect_save_header(bytes).unwrap();
        assert_eq!(header.save_version, 57);
        assert_eq!(header.prototype_format_version, PROTOTYPE_FORMAT_VERSION);

        let mut expected = sanitized_v57_fixture_simulation();
        let mut migrated = load_from_bytes(bytes).unwrap();
        migrated.validate_state().unwrap();
        assert_eq!(migrated.state_hash(), expected.state_hash(), "at migration");
        assert!(migrated.train_routing.pending.is_empty());
        let train_id = *migrated
            .rolling_stock
            .trains
            .keys()
            .next()
            .expect("the historical fixture has one train");
        assert!(
            migrated
                .train(train_id)
                .expect("the migrated train exists")
                .route_search_exhausted_at
                .is_some(),
            "the v57 exhaustion marker is preserved"
        );

        for relative_tick in 0..32 {
            expected.tick();
            migrated.tick();
            assert_eq!(
                migrated.state_hash(),
                expected.state_hash(),
                "continuation diverged at relative tick {relative_tick}"
            );
            migrated.validate_state().unwrap();
        }
        let train = migrated.train(train_id).expect("the migrated train exists");
        assert!(
            train.route.is_some(),
            "v58 restarted the frontier-less search"
        );
        assert_eq!(train.route_search_exhausted_at, None);
    }

    #[test]
    fn unknown_future_version_is_rejected_before_payload_decode() {
        let fixture = include_bytes!("../../tests/fixtures/save-v57-sanitized.factsim");
        let mut future_header = fixture[..SAVE_HEADER_SIZE].to_vec();
        future_header[8..12].copy_from_slice(&(SAVE_VERSION + 1).to_le_bytes());
        assert!(matches!(
            load_from_bytes(&future_header),
            Err(SaveLoadError::UnsupportedSaveVersion {
                found,
                supported: SAVE_VERSION,
            }) if found == SAVE_VERSION + 1
        ));
    }

    impl Read for FailingIo {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::PermissionDenied.into())
        }
    }

    impl Write for FailingIo {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::PermissionDenied.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn byte_helpers_and_fragmented_streams_are_equivalent() {
        let mut sim = Simulation::new_test_world(295);
        for _ in 0..12 {
            sim.tick();
        }
        let expected = save_to_bytes(&sim).unwrap();
        let mut writer = FragmentedWriter::default();
        save_to_writer(&sim, &mut writer).unwrap();
        assert_eq!(writer.bytes, expected);

        let mut reader = FragmentedReader {
            bytes: io::Cursor::new(expected),
            interrupt_next: true,
        };
        let loaded = load_from_reader(&mut reader).unwrap();
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }

    #[test]
    fn streaming_reports_truncation_trailing_bytes_and_io_failures() {
        let sim = Simulation::new_test_world(296);
        let bytes = save_to_bytes(&sim).unwrap();
        let mut truncated = io::Cursor::new(&bytes[..bytes.len() - 1]);
        assert!(matches!(
            load_from_reader(&mut truncated),
            Err(SaveLoadError::Codec(_))
        ));

        let mut with_trailing = bytes.clone();
        with_trailing.push(0);
        assert!(matches!(
            load_from_reader(&mut io::Cursor::new(with_trailing)),
            Err(SaveLoadError::Codec(_))
        ));
        assert!(matches!(
            load_from_reader(&mut FailingIo),
            Err(SaveLoadError::Codec(_))
        ));
        assert!(matches!(
            save_to_writer(&sim, &mut FailingIo),
            Err(SaveLoadError::Codec(_))
        ));
    }

    /// Invalid chunk shape fails before constructing a generator from its tiles.
    #[test]
    fn malformed_chunk_is_rejected_before_world_reconstruction() {
        let mut sim = Simulation::new_test_world(293);
        let (&coord, chunk) = sim.world.chunks.iter_mut().next().unwrap();
        chunk.tiles.clear();
        assert!(matches!(load_from_bytes(&save_to_bytes(&sim).unwrap()),
            Err(SaveLoadError::InvalidSimulationState(SimValidationError::InvalidChunk(found))) if found == coord));
    }

    /// The version immediately before the supported window remains rejected.
    #[test]
    fn version_before_migration_window_is_an_explicit_history_boundary() {
        let mut bytes = save_to_bytes(&Simulation::new_test_world(293)).unwrap();
        bytes[8..12].copy_from_slice(&(OLDEST_SUPPORTED_SAVE_VERSION - 1).to_le_bytes());
        bytes.truncate(SAVE_HEADER_SIZE);
        assert!(matches!(
            load_from_bytes(&bytes),
            Err(SaveLoadError::UnsupportedSaveVersion {
                found,
                supported: SAVE_VERSION
            }) if found == OLDEST_SUPPORTED_SAVE_VERSION - 1
        ));
    }

    #[test]
    fn encoder_enforces_the_loaders_payload_ceiling() {
        // Vec's fixed-width length prefix consumes eight payload bytes.
        let limits = SaveLimits {
            max_decoded_bytes: 32,
            ..SaveLimits::default()
        };
        let mut payload = vec![0_u8; 24];
        let bytes = encode_snapshot_with_limits(0, &payload, limits).unwrap();
        assert_eq!(bytes.len() as u64, 32 + SAVE_HEADER_SIZE as u64);
        drop(bytes);
        payload.push(0);
        assert!(matches!(
            encode_snapshot_with_limits(0, &payload, limits),
            Err(SaveLoadError::TooLarge)
        ));
    }

    #[test]
    fn bounded_capture_rejects_before_creating_a_generation() {
        let sim = Simulation::new_test_world(294);
        let limits = SaveLimits {
            max_decoded_bytes: 1,
            max_record_bytes: 1,
            ..SaveLimits::default()
        };

        assert!(matches!(
            try_capture_save_snapshot_with_limits(&sim, 7, limits),
            Err(SaveLoadError::TooLarge)
        ));
    }

    #[test]
    fn supported_snapshot_round_trips_at_exact_limit() {
        let sim = Simulation::new_test_world(292);
        let state = SimulationSnapshotRef::from_simulation(&sim);
        let bytes = save_to_bytes(&sim).unwrap();
        let limits = SaveLimits {
            max_encoded_bytes: bytes.len() as u64,
            max_decoded_bytes: (bytes.len() - SAVE_HEADER_SIZE) as u64,
            ..SaveLimits::default()
        };
        assert_eq!(
            encode_snapshot_with_limits(prototype_hash(&sim.world.prototypes), &state, limits)
                .unwrap(),
            bytes
        );
        let restored = load_from_bytes_with_limits(&bytes, limits).unwrap();
        assert_eq!(restored.state_hash(), sim.state_hash());
        restored.validate_state().unwrap();
        for smaller in [
            SaveLimits {
                max_encoded_bytes: limits.max_encoded_bytes - 1,
                ..limits
            },
            SaveLimits {
                max_decoded_bytes: limits.max_decoded_bytes - 1,
                ..limits
            },
            SaveLimits {
                max_record_bytes: limits.max_decoded_bytes - 1,
                ..limits
            },
        ] {
            assert!(matches!(
                encode_snapshot_with_limits(0, &state, smaller),
                Err(SaveLoadError::TooLarge)
            ));
            assert!(matches!(
                load_from_bytes_with_limits(&bytes, smaller),
                Err(SaveLoadError::TooLarge)
            ));
        }
    }

    #[test]
    fn load_rejects_corrupt_bytes() {
        let result = load_from_bytes(&[0, 1, 2, 3]);

        assert!(matches!(result, Err(SaveLoadError::Codec(_))));
    }

    #[test]
    fn load_rejects_invalid_magic_before_snapshot_decode() {
        let sim = Simulation::new_test_world(123);
        let mut bytes = save_to_bytes(&sim).unwrap();
        bytes[0] = b'X';
        bytes.truncate(SAVE_HEADER_SIZE + 1);

        let result = load_from_bytes(&bytes);

        assert!(matches!(
            result,
            Err(SaveLoadError::InvalidMagic { found }) if found[0] == b'X'
        ));
    }

    #[test]
    fn prototype_hash_changes_when_catalog_changes() {
        let mut catalog = PrototypeCatalog::load_base().unwrap();
        let before = prototype_hash(&catalog);

        catalog.items_mut()[0].stack_size += 1;

        assert_ne!(before, prototype_hash(&catalog));
    }

    #[test]
    fn prototype_hash_includes_payload_launch_products() {
        let mut catalog = PrototypeCatalog::load_base().unwrap();
        let before = prototype_hash(&catalog);
        let satellite = factory_data::item_id_by_name(&catalog, "satellite");

        catalog.items_mut()[satellite.index()].launch_products[0].amount -= 1;

        assert_ne!(before, prototype_hash(&catalog));
    }

    #[test]
    fn round_trip_preserves_tick_seed_and_hash() {
        let mut sim = Simulation::new_test_world(8675309);
        for _ in 0..128 {
            sim.tick();
        }

        let before_hash = sim.state_hash();
        let bytes = save_to_bytes(&sim).unwrap();
        let loaded = load_from_bytes(&bytes).unwrap();

        assert_eq!(sim.tick_count(), loaded.tick_count());
        assert_eq!(sim.seed(), loaded.seed());
        assert_eq!(before_hash, loaded.state_hash());
    }

    #[test]
    fn delayed_combat_round_trip_stays_lockstep_through_effect_ticks() {
        let mut original = Simulation::new_test_world(8675309);
        let target = CombatantId::Player;
        let source = CombatSource::new(CombatantId::Enemy(EnemyId::new(u64::MAX)), Faction::Enemy);
        let (impact_x, impact_y) = original.player.tile_position();
        let projectile_id = ProjectileId::new(0);
        original.delayed_combat = DelayedCombatState {
            next_projectile_id: 1,
            projectiles: BTreeMap::from([(
                projectile_id,
                ProjectileState {
                    id: projectile_id,
                    source,
                    start_x_fixed: original.player.x_fixed() - POSITION_SCALE,
                    start_y_fixed: original.player.y_fixed(),
                    impact_x,
                    impact_y,
                    launched_tick: original.tick,
                    impact_tick: original.tick + 4,
                    speed_fixed_per_tick: 256,
                    damage: Damage::new(7, DamageType::Explosion),
                    explosion_radius_tiles: 1,
                },
            )]),
            statuses: BTreeMap::from([(
                target,
                CombatStatusEffects {
                    burning: Some(BurningStatus {
                        source,
                        damage_per_tick: Damage::new(3, DamageType::Fire),
                        interval_ticks: 2,
                        next_damage_tick: original.tick + 2,
                        expires_tick: original.tick + 6,
                    }),
                },
            )]),
        };
        original
            .validate()
            .expect("delayed combat fixture is valid");

        let bytes = save_to_bytes(&original).expect("delayed combat state should save");
        let mut restored = load_from_bytes(&bytes).expect("delayed combat state should reload");

        assert_eq!(restored.delayed_combat, original.delayed_combat);
        assert_eq!(restored.state_hash(), original.state_hash());

        for expected_health in [100, 97, 97, 87, 87, 84] {
            original.tick();
            restored.tick();
            assert_eq!(restored.player_health().0, expected_health);
            assert_eq!(restored.delayed_combat, original.delayed_combat);
            assert_eq!(restored.state_hash(), original.state_hash());
        }
        assert!(original.delayed_combat.projectiles.is_empty());
        assert!(original.delayed_combat.statuses.is_empty());
    }

    #[test]
    fn round_trip_preserves_pending_chunk_generation_order() {
        let mut sim = Simulation::new_test_world(123);
        let required = ChunkCoord { x: 40, y: -37 };
        let prefetch = ChunkCoord { x: -30, y: 31 };
        sim.request_chunk_generation(prefetch, ChunkGenerationPriority::Prefetch);
        sim.request_chunk_generation(required, ChunkGenerationPriority::Required);

        let bytes = save_to_bytes(&sim).unwrap();
        let mut loaded = load_from_bytes(&bytes).unwrap();

        assert_eq!(sim.state_hash(), loaded.state_hash());
        assert_eq!(loaded.process_chunk_generation_queue(1), 1);
        assert!(loaded.world.chunks.contains_key(&required));
        assert!(!loaded.world.chunks.contains_key(&prefetch));
        assert_save_continuation(&mut sim, 8, &[]);
        assert!(sim.world.chunks.contains_key(&required));
        assert!(sim.world.chunks.contains_key(&prefetch));
    }

    #[test]
    fn save_header_layout_matches_loader() {
        let sim = Simulation::new_test_world(42);
        let bytes = save_to_bytes(&sim).expect("save should serialize");

        assert_eq!(&bytes[..8], &SAVE_MAGIC);
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            SAVE_VERSION
        );
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            PROTOTYPE_FORMAT_VERSION
        );
        assert!(load_from_bytes(&bytes).is_ok());
    }

    #[test]
    fn header_inspection_reports_versions_without_rejecting_them() {
        let sim = Simulation::new_test_world(42);
        let bytes = save_to_bytes(&sim).unwrap();
        let expected = inspect_save_header(&bytes).unwrap();
        assert_eq!(expected.save_version, SAVE_VERSION);

        for version in [SAVE_VERSION - 1, SAVE_VERSION + 1] {
            let mut changed = bytes[..SAVE_HEADER_SIZE].to_vec();
            changed[8..12].copy_from_slice(&version.to_le_bytes());
            assert_eq!(inspect_save_header(&changed).unwrap().save_version, version);
        }
    }

    #[test]
    fn header_inspection_rejects_truncation_and_invalid_magic() {
        assert!(matches!(
            inspect_save_header(&[0; SAVE_HEADER_SIZE - 1]),
            Err(SaveLoadError::Codec(_))
        ));
        let mut header = [0; SAVE_HEADER_SIZE];
        header[..8].copy_from_slice(b"NOTASAVE");
        assert!(matches!(
            inspect_save_header(&header),
            Err(SaveLoadError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn save_load_preserves_generated_chunks_and_future_generation() {
        let mut sim = Simulation::new_test_world(123);
        let far = ChunkCoord { x: 30, y: -24 };
        let future = ChunkCoord { x: -41, y: 37 };
        sim.world.ensure_chunk_generated(far);
        let before_hash = sim.state_hash();
        let before_coords = sim.world.chunks.keys().copied().collect::<BTreeSet<_>>();

        let bytes = save_to_bytes(&sim).unwrap();
        let mut loaded = load_from_bytes(&bytes).unwrap();

        assert_eq!(
            sim.world.generated_chunk_count(),
            loaded.world.generated_chunk_count()
        );
        assert_eq!(
            before_coords,
            loaded.world.chunks.keys().copied().collect::<BTreeSet<_>>()
        );
        assert_eq!(before_hash, loaded.state_hash());
        sim.world.ensure_chunk_generated(future);
        loaded.world.ensure_chunk_generated(future);
        assert_eq!(
            sim.world.chunks.get(&future),
            loaded.world.chunks.get(&future)
        );
    }

    #[test]
    fn save_after_one_far_chunk_does_not_load_unrelated_far_chunks() {
        let mut sim = Simulation::new_test_world(123);
        let far = ChunkCoord { x: 80, y: 80 };
        let unrelated = ChunkCoord { x: 81, y: 80 };
        sim.world.ensure_chunk_generated(far);

        let loaded = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();

        assert!(loaded.world.chunks.contains_key(&far));
        assert!(!loaded.world.chunks.contains_key(&unrelated));
        assert_eq!(loaded.world.generated_chunk_count(), 26);
    }

    #[test]
    fn generated_twenty_by_twenty_world_validates_and_round_trips() {
        let mut sim = Simulation::new_test_world(123);
        for y in -10..10 {
            for x in -10..10 {
                sim.world.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
        sim.validate_state().unwrap();
        let hash = sim.state_hash();

        let loaded = load_from_bytes(&save_to_bytes(&sim).unwrap()).unwrap();

        assert_eq!(
            loaded.world.generated_chunk_count(),
            sim.world.generated_chunk_count()
        );
        assert_eq!(hash, loaded.state_hash());
    }

    /// Dirty topology always has empty published summaries; accepting both a
    /// dirty marker and summaries would make the load boundary ambiguous.
    #[test]
    fn pending_network_invalidation_rejects_published_summaries() {
        let sim = Simulation::new_test_world(293);

        let mut fluid = capture_save_snapshot(&sim);
        fluid
            .state
            .fluid_networks
            .push(FluidNetworkSnapshot::default());
        assert!(matches!(
            load_from_bytes(&save_snapshot_to_bytes(&fluid).unwrap()),
            Err(SaveLoadError::InvalidSimulationState(
                SimValidationError::InvalidFluidNetwork { .. }
            ))
        ));

        let mut heat = capture_save_snapshot(&sim);
        heat.state
            .heat_networks
            .push(HeatNetworkSnapshot::default());
        assert!(matches!(
            load_from_bytes(&save_snapshot_to_bytes(&heat).unwrap()),
            Err(SaveLoadError::InvalidSimulationState(
                SimValidationError::InvalidHeatNetwork { .. }
            ))
        ));
    }

    #[test]
    /// Verifies an owned snapshot cannot drift as the live simulation advances.
    fn owned_save_snapshot_remains_at_its_captured_completed_tick() {
        let mut sim = Simulation::new_test_world(123);
        for _ in 0..3 {
            sim.tick();
        }
        let captured_tick = sim.tick_count();
        let captured_hash = sim.state_hash();
        let snapshot = capture_save_snapshot(&sim);

        sim.tick();
        let loaded = load_from_bytes(&save_snapshot_to_bytes(&snapshot).unwrap()).unwrap();

        assert_eq!(snapshot.tick_count(), captured_tick);
        assert_eq!(loaded.tick_count(), captured_tick);
        assert_eq!(loaded.state_hash(), captured_hash);
        assert_ne!(loaded.state_hash(), sim.state_hash());
    }
}
