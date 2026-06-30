use crate::world::{ResourceCell, TileCollision};
use factory_data::TileId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub tiles: Vec<TileCell>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TileCell {
    pub tile_id: TileId,
    pub collision: TileCollision,
    pub resource: Option<ResourceCell>,
}
