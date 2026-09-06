use crate::ids::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Eq, Serialize)]
pub struct OccupancyGrid {
    /// Runtime invalidation only; rebuilding the same grid preserves identity.
    #[serde(skip)]
    pub(crate) revision: u64,
    // maps occupied tile -> entity id
    pub(crate) occupied_tiles:
        BTreeMap<(crate::world::WorldTileCoord, crate::world::WorldTileCoord), EntityId>,
}

impl PartialEq for OccupancyGrid {
    fn eq(&self, other: &Self) -> bool {
        self.occupied_tiles == other.occupied_tiles
    }
}

impl std::hash::Hash for OccupancyGrid {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.occupied_tiles, state);
    }
}
