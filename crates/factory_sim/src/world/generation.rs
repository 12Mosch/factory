use crate::simulation::CachedWorldGenRules;
use crate::world::{Chunk, ChunkCoord, ResourceTileChange};
use factory_data::PrototypeCatalog;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};

/// Runtime-only cache of generation rules derived from the prototype catalog.
///
/// The cache is deliberately excluded from world identity: it can be empty
/// immediately after direct deserialization and populated lazily without
/// changing simulation behavior.
#[derive(Clone, Debug, Default)]
pub(crate) struct GenerationRulesCache(pub(crate) Option<CachedWorldGenRules>);

impl PartialEq for GenerationRulesCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for GenerationRulesCache {}

impl Hash for GenerationRulesCache {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
    /// Derived prototype lookup used while generating chunk absorption
    /// totals. Prototype and terrain mutation must rebuild the affected
    /// caches if either is introduced in the future.
    #[serde(skip, default)]
    pub(crate) tile_pollution_absorption_per_minute_milli: Vec<u64>,
    /// Resolved terrain and resource rules reused by streamed chunk
    /// generation. Prototype mutation must invalidate this cache if it is
    /// introduced in the future.
    #[serde(skip, default)]
    pub(crate) generation_rules: GenerationRulesCache,
    #[serde(skip, default)]
    pub(crate) chunk_revision: u64,
    pub(crate) resource_revision: u64,
    #[serde(skip, default)]
    pub(crate) resource_dirty_tiles: VecDeque<ResourceTileChange>,
}
