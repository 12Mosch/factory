use crate::simulation::WorldGenerator;
use crate::world::{Chunk, ChunkCoord, ResourceTileChange};
use factory_data::PrototypeCatalog;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Exact coordinates inserted by one chunk-generation operation.
///
/// The coordinates retain the caller's iteration order and never contain
/// duplicates or chunks that already existed. `revision` is the world chunk
/// revision after the operation completed, allowing deferred consumers such
/// as rendering to continue from the same generation boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChunkGenerationResult {
    revision: u64,
    generated_chunks: Vec<ChunkCoord>,
}

impl ChunkGenerationResult {
    pub(crate) fn new(revision: u64) -> Self {
        Self {
            revision,
            generated_chunks: Vec::new(),
        }
    }

    pub(crate) fn from_generated_chunks(
        revision: u64,
        chunks: impl IntoIterator<Item = ChunkCoord>,
    ) -> Self {
        Self {
            revision,
            generated_chunks: chunks.into_iter().collect(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn generated_chunks(&self) -> &[ChunkCoord] {
        &self.generated_chunks
    }

    pub fn into_generated_chunks(self) -> Vec<ChunkCoord> {
        self.generated_chunks
    }

    pub fn len(&self) -> usize {
        self.generated_chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.generated_chunks.is_empty()
    }

    pub fn contains(&self, coord: ChunkCoord) -> bool {
        self.generated_chunks.contains(&coord)
    }

    pub(crate) fn push(&mut self, coord: ChunkCoord, revision: u64) {
        self.generated_chunks.push(coord);
        self.revision = revision;
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChunkGenerationChange {
    pub(crate) revision: u64,
    pub(crate) coord: ChunkCoord,
}

/// Runtime-only bounded change history used by deferred presentation
/// consumers. It does not participate in durable world identity.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChunkGenerationHistory(pub(crate) VecDeque<ChunkGenerationChange>);

impl PartialEq for ChunkGenerationHistory {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for ChunkGenerationHistory {}

impl std::hash::Hash for ChunkGenerationHistory {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

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
    #[serde(skip, default)]
    pub(crate) chunk_generation_history: ChunkGenerationHistory,
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
