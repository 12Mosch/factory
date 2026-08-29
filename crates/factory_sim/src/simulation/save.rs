use super::*;
use bincode::Options;

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
pub const SAVE_VERSION: u32 = 49;
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
pub const PROTOTYPE_FORMAT_VERSION: u32 = 32;

const SAVE_MAGIC: [u8; 8] = *b"FACTSIM\0";
pub const SAVE_HEADER_SIZE: usize = 8 + 4 + 4 + 8;
const MAX_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum SaveLoadError {
    Codec(Box<bincode::ErrorKind>),
    InvalidMagic { found: [u8; 8] },
    UnsupportedSaveVersion { found: u32, supported: u32 },
    UnsupportedPrototypeFormatVersion { found: u32, supported: u32 },
    PrototypeHashMismatch { stored: u64, computed: u64 },
    InvalidSimulationState(SimulationValidationError),
}

impl From<bincode::Error> for SaveLoadError {
    fn from(error: bincode::Error) -> Self {
        Self::Codec(error)
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

#[derive(Clone, Deserialize, Serialize)]
struct SimulationSnapshotOwned {
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
    entities: EntityStore,
    construction: ConstructionState,
    player: PlayerState,
    player_equipment: PlayerEquipmentState,
    player_inventory: Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    onboarding_progress: OnboardingProgress,
    research: ResearchState,
    power_summary: PowerSummary,
    power_networks: Vec<PowerNetworkSnapshot>,
    entity_power_statuses: DenseEntityMap<EntityPowerStatus>,
    fluid_networks: Vec<FluidNetworkSnapshot>,
    heat_networks: Vec<HeatNetworkSnapshot>,
    robot_networks: Vec<RobotNetworkSnapshot>,
    robot_flights: RobotFlightSubsystem,
    rolling_stock: RollingStockSubsystem,
    pollution: PollutionState,
    enemies: EnemySubsystem,
    config: SimulationConfig,
}

/// An owned, immutable copy of the durable state for one completed simulation tick.
///
/// Capturing the snapshot performs the state copies needed to release the live
/// simulation immediately. Encoding can then happen on another thread without
/// borrowing or locking the simulation.
pub struct SimulationSaveSnapshot {
    prototype_hash: u64,
    state: SimulationSnapshotOwned,
}

impl SimulationSaveSnapshot {
    /// Returns the completed simulation tick represented by this snapshot.
    pub fn tick_count(&self) -> u64 {
        self.state.tick
    }
}

/// Captures the durable state at the simulation's current completed-tick boundary.
pub fn capture_save_snapshot(sim: &Simulation) -> SimulationSaveSnapshot {
    SimulationSaveSnapshot {
        prototype_hash: prototype_hash(&sim.world.prototypes),
        state: SimulationSnapshotOwned::from_simulation(sim),
    }
}

/// Serializes a previously captured snapshot without accessing the live simulation.
pub fn save_snapshot_to_bytes(snapshot: &SimulationSaveSnapshot) -> Result<Vec<u8>, SaveLoadError> {
    encode_snapshot(snapshot.prototype_hash, &snapshot.state)
}

pub fn save_to_bytes(sim: &Simulation) -> Result<Vec<u8>, SaveLoadError> {
    let prototype_hash = prototype_hash(&sim.world.prototypes);
    let snapshot = SimulationSnapshotRef::from_simulation(sim);
    encode_snapshot(prototype_hash, &snapshot)
}

/// Encodes either borrowed or owned durable state with the common save header.
fn encode_snapshot(
    prototype_hash: u64,
    snapshot: &impl Serialize,
) -> Result<Vec<u8>, SaveLoadError> {
    let mut bytes = Vec::with_capacity(SAVE_HEADER_SIZE);
    bytes.extend_from_slice(&SAVE_MAGIC);
    bytes.extend_from_slice(&SAVE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&PROTOTYPE_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&prototype_hash.to_le_bytes());
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize_into(&mut bytes, snapshot)
        .map_err(SaveLoadError::from)?;
    Ok(bytes)
}

pub fn load_from_bytes(bytes: &[u8]) -> Result<Simulation, SaveLoadError> {
    let (header, snapshot_bytes) = read_header(bytes)?;

    if header.magic != SAVE_MAGIC {
        return Err(SaveLoadError::InvalidMagic {
            found: header.magic,
        });
    }
    if header.save_version != SAVE_VERSION {
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

    if snapshot_bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(size_limit_error());
    }

    let snapshot: SimulationSnapshotOwned = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_limit(MAX_SNAPSHOT_BYTES)
        .deserialize(snapshot_bytes)
        .map_err(SaveLoadError::from)?;
    let computed_hash = prototype_hash(&snapshot.prototypes);
    if header.prototype_hash != computed_hash {
        return Err(SaveLoadError::PrototypeHashMismatch {
            stored: header.prototype_hash,
            computed: computed_hash,
        });
    }

    let sim = snapshot.into_simulation();
    sim.validate_state()
        .map_err(SaveLoadError::InvalidSimulationState)?;
    Ok(sim)
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
pub fn inspect_save_header(bytes: &[u8]) -> Result<SaveHeaderInfo, SaveLoadError> {
    let (header, _) = read_header(bytes)?;
    if header.magic != SAVE_MAGIC {
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

fn size_limit_error() -> SaveLoadError {
    SaveLoadError::Codec(bincode::ErrorKind::SizeLimit.into())
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

#[derive(Serialize)]
struct SimulationSnapshotRef<'a> {
    tick: u64,
    day_night_cycle: Option<DayNightCycleState>,
    world_seed: u64,
    prototypes: &'a PrototypeCatalog,
    chunks: &'a BTreeMap<ChunkCoord, Chunk>,
    chunk_generation_queue: &'a ChunkGenerationQueue,
    chart: &'a ChartState,
    item_statistics: &'a ItemStatistics,
    fluid_statistics: &'a FluidStatistics,
    power_statistics: &'a PowerStatistics,
    rockets_launched: u64,
    entities: &'a EntityStore,
    construction: &'a ConstructionState,
    player: PlayerState,
    player_equipment: &'a PlayerEquipmentState,
    player_inventory: &'a Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: &'a CraftingQueue,
    onboarding_progress: OnboardingProgress,
    research: &'a ResearchState,
    power_summary: PowerSummary,
    power_networks: &'a Vec<PowerNetworkSnapshot>,
    entity_power_statuses: &'a DenseEntityMap<EntityPowerStatus>,
    fluid_networks: &'a Vec<FluidNetworkSnapshot>,
    heat_networks: &'a Vec<HeatNetworkSnapshot>,
    robot_networks: &'a Vec<RobotNetworkSnapshot>,
    robot_flights: &'a RobotFlightSubsystem,
    rolling_stock: &'a RollingStockSubsystem,
    pollution: &'a PollutionState,
    enemies: &'a EnemySubsystem,
    config: SimulationConfig,
}

impl<'a> SimulationSnapshotRef<'a> {
    fn from_simulation(sim: &'a Simulation) -> Self {
        Self {
            tick: sim.tick,
            day_night_cycle: sim.day_night_cycle,
            world_seed: sim.world.seed,
            prototypes: &sim.world.prototypes,
            chunks: &sim.world.chunks,
            chunk_generation_queue: &sim.chunk_generation_queue,
            chart: &sim.chart,
            item_statistics: &sim.statistics.items,
            fluid_statistics: &sim.statistics.fluids,
            power_statistics: &sim.statistics.power,
            rockets_launched: sim.statistics.rockets_launched,
            entities: &sim.entities,
            construction: &sim.construction,
            player: sim.player,
            player_equipment: &sim.player_equipment,
            player_inventory: &sim.player_inventory,
            manual_mining_progress: sim.manual_mining_progress,
            crafting_queue: &sim.crafting_queue,
            onboarding_progress: sim.onboarding_progress,
            research: &sim.research,
            power_summary: sim.power.summary,
            power_networks: &sim.power.networks,
            entity_power_statuses: &sim.power.entity_statuses,
            fluid_networks: &sim.fluids.networks,
            heat_networks: &sim.heat.networks,
            robot_networks: &sim.robots.networks,
            robot_flights: &sim.robot_flights,
            rolling_stock: &sim.rolling_stock,
            pollution: &sim.pollution,
            enemies: &sim.enemies,
            config: sim.config,
        }
    }
}

impl SimulationSnapshotOwned {
    /// Copies only durable save state, excluding all reconstructible runtime caches.
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
            entities: sim.entities.clone(),
            construction: sim.construction.clone(),
            player: sim.player,
            player_equipment: sim.player_equipment.clone(),
            player_inventory: sim.player_inventory.clone(),
            manual_mining_progress: sim.manual_mining_progress,
            crafting_queue: sim.crafting_queue.clone(),
            onboarding_progress: sim.onboarding_progress,
            research: sim.research.clone(),
            power_summary: sim.power.summary,
            power_networks: sim.power.networks.clone(),
            entity_power_statuses: sim.power.entity_statuses.clone(),
            fluid_networks: sim.fluids.networks.clone(),
            heat_networks: sim.heat.networks.clone(),
            robot_networks: sim.robots.networks.clone(),
            robot_flights: sim.robot_flights.clone(),
            rolling_stock: sim.rolling_stock.clone(),
            pollution: sim.pollution.clone(),
            enemies: sim.enemies.clone(),
            config: sim.config,
        }
    }

    fn into_simulation(self) -> Simulation {
        let mut sim = Simulation {
            tick: self.tick,
            day_night_cycle: self.day_night_cycle,
            entity_topology_revision: 0,
            revealed_revision: 0,
            revealed_chunk_history: Default::default(),
            pollution_map_revision: 0,
            enemy_map_revision: 0,
            power_map_revision: 0,
            production_status_revision: 0,
            production_map_statuses: Vec::new(),
            production_map_status_scratch: Vec::new(),
            world: WorldSim::from_snapshot(self.world_seed, self.prototypes, self.chunks),
            chunk_generation_queue: self.chunk_generation_queue,
            chart: self.chart,
            entities: self.entities,
            construction: self.construction,
            player: self.player,
            player_equipment: self.player_equipment,
            player_inventory: self.player_inventory,
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
            train_routing: rolling_stock_ops::TrainRouting::default(),
            stopped_stock_index: rolling_stock_ops::StoppedStockIndex::default(),
            robots: RobotSubsystem::from_networks(self.robot_networks),
            robot_flights: self.robot_flights,
            rolling_stock: self.rolling_stock,
            circuits: CircuitSubsystem::default(),
            statistics: StatisticsSubsystem {
                items: self.item_statistics,
                fluids: self.fluid_statistics,
                power: self.power_statistics,
                rockets_launched: self.rockets_launched,
            },
            pollution: self.pollution,
            capacity_overflows: CapacityOverflowCounters::default(),
            pollution_emitters: PollutionEmitterIndex::default(),
            pollution_diffusion: PollutionDiffusionBuffer::default(),
            enemies: self.enemies,
            config: self.config,
            attack_targets: enemy::AttackTargetCache::default(),
            enemy_target_chunks: combat_ops::EnemyChunkIndex::default(),
            enemy_spawning_scratch: enemy::EnemySpawningScratch::default(),
            enemy_navigation: enemy::EnemyNavigation::default(),
            transport: TransportLaneCache::default(),
        };
        sim.transport.initialize_item_tracking(&sim.entities);
        // The rail graph is a derived cache like the circuit topology, so a
        // loaded world rebuilds it before anything can ask what connects — and
        // before the stopped-stock index, which is read off the geometry it
        // holds.
        sim.ensure_rail_graph();
        // Ahead of the fluid topology, because a stopped fluid wagon is part of
        // the network at the pump it is standing at: a topology built before the
        // index would leave the wagon out, and the saved network snapshots —
        // taken from a world that had it in — would not describe it.
        sim.refresh_stopped_stock_index();
        sim.ensure_fluid_network_topology();
        // The snapshots are re-derived rather than trusted, because the index
        // above may have joined a wagon onto a network and cleared them. A valid
        // save re-derives exactly what it stored: the topology is a function of
        // the same entities, rails, and stock positions it was saved from.
        sim.refresh_fluid_network_snapshots();
        // Robot coverage queries read the topology cache, so rebuild it before
        // anything can ask a loaded world which network covers a tile.
        sim.ensure_robot_network_topology();
        sim.rebuild_circuit_state();
        sim.rebuild_all_module_effects();
        sim.rebuild_pollution_emitter_index();
        sim
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

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

        catalog.items[0].stack_size += 1;

        assert_ne!(before, prototype_hash(&catalog));
    }

    #[test]
    fn prototype_hash_includes_payload_launch_products() {
        let mut catalog = PrototypeCatalog::load_base().unwrap();
        let before = prototype_hash(&catalog);
        let satellite = factory_data::item_id_by_name(&catalog, "satellite");

        catalog.items[satellite.index()].launch_products[0].amount -= 1;

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
