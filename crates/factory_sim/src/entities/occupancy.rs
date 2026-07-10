use crate::ids::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OccupancyGrid {
    // maps occupied tile -> entity id
    pub(crate) occupied_tiles:
        BTreeMap<(crate::world::WorldTileCoord, crate::world::WorldTileCoord), EntityId>,
}
