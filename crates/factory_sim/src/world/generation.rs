use crate::world::{Chunk, ChunkCoord, ResourceTileChange};
use factory_data::PrototypeCatalog;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WorldSim {
    pub seed: u64,
    pub prototypes: PrototypeCatalog,
    pub chunks: BTreeMap<ChunkCoord, Chunk>,
    #[serde(skip, default)]
    pub(crate) chunk_revision: u64,
    pub(crate) resource_revision: u64,
    #[serde(skip, default)]
    pub(crate) resource_dirty_tiles: VecDeque<ResourceTileChange>,
}
