use crate::simulation::WorldGenerator;
use crate::world::{Chunk, ChunkCoord, ResourceTileChange};
use factory_data::PrototypeCatalog;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
    /// Runtime-only generation state resolved from `prototypes` once at
    /// construction or load. Prototype mutation must rebuild this generator
    /// if it is introduced in the future.
    #[serde(skip)]
    pub(crate) generator: WorldGenerator,
    #[serde(skip, default)]
    pub(crate) chunk_revision: u64,
    pub(crate) resource_revision: u64,
    #[serde(skip, default)]
    pub(crate) resource_dirty_tiles: VecDeque<ResourceTileChange>,
}

#[derive(Deserialize)]
struct SerializedWorldSim {
    seed: u64,
    prototypes: PrototypeCatalog,
    chunks: BTreeMap<ChunkCoord, Chunk>,
    #[serde(default)]
    resource_revision: u64,
}

impl<'de> Deserialize<'de> for WorldSim {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let serialized = SerializedWorldSim::deserialize(deserializer)?;
        // Direct deserialization intentionally takes the same rebuild path as
        // save loading so derived generation state and per-chunk pollution
        // absorption are immediately consistent with the prototype catalog.
        let mut world =
            Self::from_snapshot(serialized.seed, serialized.prototypes, serialized.chunks);
        world.resource_revision = serialized.resource_revision;
        Ok(world)
    }
}
