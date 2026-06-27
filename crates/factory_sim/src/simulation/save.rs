use super::*;

pub const SAVE_VERSION: u32 = 1;
pub const PROTOTYPE_FORMAT_VERSION: u32 = 1;

const SAVE_MAGIC: [u8; 8] = *b"FACTSIM\0";

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

#[derive(Deserialize, Serialize)]
struct SaveFile {
    magic: [u8; 8],
    save_version: u32,
    prototype_format_version: u32,
    prototype_hash: u64,
    snapshot: SimulationSnapshot,
}

#[derive(Deserialize, Serialize)]
struct SimulationSnapshot {
    tick: u64,
    world_seed: u64,
    prototypes: PrototypeCatalog,
    chunks: BTreeMap<ChunkCoord, Chunk>,
    entities: EntityStore,
    player: PlayerState,
    player_inventory: Inventory,
    manual_mining_progress: Option<ManualMiningProgress>,
    crafting_queue: CraftingQueue,
    research: ResearchState,
}

pub fn save_to_bytes(sim: &Simulation) -> Result<Vec<u8>, SaveLoadError> {
    let prototype_hash = prototype_hash(&sim.world.prototypes);
    let save_file = SaveFile {
        magic: SAVE_MAGIC,
        save_version: SAVE_VERSION,
        prototype_format_version: PROTOTYPE_FORMAT_VERSION,
        prototype_hash,
        snapshot: SimulationSnapshot::from_simulation(sim),
    };

    bincode::serialize(&save_file).map_err(SaveLoadError::from)
}

pub fn load_from_bytes(bytes: &[u8]) -> Result<Simulation, SaveLoadError> {
    let save_file: SaveFile = bincode::deserialize(bytes).map_err(SaveLoadError::from)?;

    if save_file.magic != SAVE_MAGIC {
        return Err(SaveLoadError::InvalidMagic {
            found: save_file.magic,
        });
    }
    if save_file.save_version != SAVE_VERSION {
        return Err(SaveLoadError::UnsupportedSaveVersion {
            found: save_file.save_version,
            supported: SAVE_VERSION,
        });
    }
    if save_file.prototype_format_version != PROTOTYPE_FORMAT_VERSION {
        return Err(SaveLoadError::UnsupportedPrototypeFormatVersion {
            found: save_file.prototype_format_version,
            supported: PROTOTYPE_FORMAT_VERSION,
        });
    }

    let computed_hash = prototype_hash(&save_file.snapshot.prototypes);
    if save_file.prototype_hash != computed_hash {
        return Err(SaveLoadError::PrototypeHashMismatch {
            stored: save_file.prototype_hash,
            computed: computed_hash,
        });
    }

    let sim = save_file.snapshot.into_simulation();
    sim.validate_state()
        .map_err(SaveLoadError::InvalidSimulationState)?;
    Ok(sim)
}

pub fn prototype_hash(catalog: &PrototypeCatalog) -> u64 {
    let mut hasher = StableHasher::default();
    "factory-prototype-catalog-v1".hash(&mut hasher);
    catalog.hash(&mut hasher);
    hasher.finish()
}

impl SimulationSnapshot {
    fn from_simulation(sim: &Simulation) -> Self {
        Self {
            tick: sim.tick,
            world_seed: sim.world.seed,
            prototypes: sim.world.prototypes.clone(),
            chunks: sim.world.chunks.clone(),
            entities: sim.entities.clone(),
            player: sim.player,
            player_inventory: sim.player_inventory.clone(),
            manual_mining_progress: sim.manual_mining_progress,
            crafting_queue: sim.crafting_queue.clone(),
            research: sim.research.clone(),
        }
    }

    fn into_simulation(self) -> Simulation {
        Simulation {
            tick: self.tick,
            world: WorldSim {
                seed: self.world_seed,
                prototypes: self.prototypes,
                chunks: self.chunks,
            },
            entities: self.entities,
            player: self.player,
            player_inventory: self.player_inventory,
            manual_mining_progress: self.manual_mining_progress,
            crafting_queue: self.crafting_queue,
            research: self.research,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_rejects_corrupt_bytes() {
        let result = load_from_bytes(&[0, 1, 2, 3]);

        assert!(matches!(result, Err(SaveLoadError::Codec(_))));
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
}
