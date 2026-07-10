use super::*;
use bincode::Options;

// Save version 9 intentionally invalidates older saves: construction planning
// became part of deterministic simulation state and no v8 migration is kept.
pub const SAVE_VERSION: u32 = 11;
pub const PROTOTYPE_FORMAT_VERSION: u32 = 7;

const SAVE_MAGIC: [u8; 8] = *b"FACTSIM\0";
const SAVE_HEADER_LEN: usize = 8 + 4 + 4 + 8;
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

#[derive(Clone, Copy)]
struct SaveHeader {
    magic: [u8; 8],
    save_version: u32,
    prototype_format_version: u32,
    prototype_hash: u64,
}

#[derive(Deserialize)]
struct SimulationSnapshotOwned {
    tick: u64,
    world_seed: u64,
    prototypes: PrototypeCatalog,
    chunks: BTreeMap<ChunkCoord, Chunk>,
    chart: ChartState,
    item_statistics: ItemStatistics,
    fluid_statistics: FluidStatistics,
    power_statistics: PowerStatistics,
    entities: EntityStore,
    construction: ConstructionState,
    player: PlayerState,
    player_inventory: Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    research: ResearchState,
    power_summary: PowerSummary,
    power_networks: Vec<PowerNetworkSnapshot>,
    entity_power_statuses: BTreeMap<EntityId, EntityPowerStatus>,
    fluid_networks: Vec<FluidNetworkSnapshot>,
}

pub fn save_to_bytes(sim: &Simulation) -> Result<Vec<u8>, SaveLoadError> {
    let prototype_hash = prototype_hash(&sim.world.prototypes);
    let mut bytes = Vec::with_capacity(SAVE_HEADER_LEN);
    bytes.extend_from_slice(&SAVE_MAGIC);
    bytes.extend_from_slice(&SAVE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&PROTOTYPE_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&prototype_hash.to_le_bytes());
    let snapshot = SimulationSnapshotRef::from_simulation(sim);
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize_into(&mut bytes, &snapshot)
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
    if bytes.len() < SAVE_HEADER_LEN {
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

    Ok((header, &bytes[SAVE_HEADER_LEN..]))
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
    world_seed: u64,
    prototypes: &'a PrototypeCatalog,
    chunks: &'a BTreeMap<ChunkCoord, Chunk>,
    chart: &'a ChartState,
    item_statistics: &'a ItemStatistics,
    fluid_statistics: &'a FluidStatistics,
    power_statistics: &'a PowerStatistics,
    entities: &'a EntityStore,
    construction: &'a ConstructionState,
    player: PlayerState,
    player_inventory: &'a Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: &'a CraftingQueue,
    research: &'a ResearchState,
    power_summary: PowerSummary,
    power_networks: &'a Vec<PowerNetworkSnapshot>,
    entity_power_statuses: &'a BTreeMap<EntityId, EntityPowerStatus>,
    fluid_networks: &'a Vec<FluidNetworkSnapshot>,
}

impl<'a> SimulationSnapshotRef<'a> {
    fn from_simulation(sim: &'a Simulation) -> Self {
        Self {
            tick: sim.tick,
            world_seed: sim.world.seed,
            prototypes: &sim.world.prototypes,
            chunks: &sim.world.chunks,
            chart: &sim.chart,
            item_statistics: &sim.statistics.items,
            fluid_statistics: &sim.statistics.fluids,
            power_statistics: &sim.statistics.power,
            entities: &sim.entities,
            construction: &sim.construction,
            player: sim.player,
            player_inventory: &sim.player_inventory,
            manual_mining_progress: sim.manual_mining_progress,
            crafting_queue: &sim.crafting_queue,
            research: &sim.research,
            power_summary: sim.power.summary,
            power_networks: &sim.power.networks,
            entity_power_statuses: &sim.power.entity_statuses,
            fluid_networks: &sim.fluids.networks,
        }
    }
}

impl SimulationSnapshotOwned {
    fn into_simulation(self) -> Simulation {
        let mut sim = Simulation {
            tick: self.tick,
            entity_topology_revision: 0,
            revealed_revision: 0,
            world: WorldSim {
                seed: self.world_seed,
                prototypes: self.prototypes,
                chunks: self.chunks,
                chunk_revision: 0,
                resource_revision: 0,
                resource_dirty_tiles: VecDeque::new(),
            },
            chart: self.chart,
            entities: self.entities,
            construction: self.construction,
            player: self.player,
            player_inventory: self.player_inventory,
            manual_mining_progress: self.manual_mining_progress,
            crafting_queue: self.crafting_queue,
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
            fluids: FluidSubsystem::from_networks(self.fluid_networks),
            statistics: StatisticsSubsystem {
                items: self.item_statistics,
                fluids: self.fluid_statistics,
                power: self.power_statistics,
            },
            transport: TransportLaneCache::default(),
        };
        sim.ensure_fluid_network_topology();
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
        bytes.truncate(SAVE_HEADER_LEN + 1);

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
}
