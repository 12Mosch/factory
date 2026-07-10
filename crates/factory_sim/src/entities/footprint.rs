use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityFootprint {
    pub x: crate::world::WorldTileCoord,
    pub y: crate::world::WorldTileCoord,
    pub width: i32,
    pub height: i32,
}
