use crate::world::{ResourceCell, TileCollision};
use factory_data::TileId;
use serde::{Deserialize, Serialize};

/// Absolute tile coordinate in the generated world. Chunk-local dimensions and
/// offsets deliberately remain `i32`; only positions in the world plane use
/// this wider semantic type.
pub type WorldTileCoord = i64;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl ChunkCoord {
    /// The absolute tile at this chunk's lower-left corner.
    pub const fn min_tile(self) -> (WorldTileCoord, WorldTileCoord) {
        (
            self.x as WorldTileCoord * crate::simulation::CHUNK_SIZE as WorldTileCoord,
            self.y as WorldTileCoord * crate::simulation::CHUNK_SIZE as WorldTileCoord,
        )
    }

    /// Converts a bounded chunk-local offset to an absolute tile coordinate.
    pub const fn tile_at(self, local_x: i32, local_y: i32) -> (WorldTileCoord, WorldTileCoord) {
        let (x, y) = self.min_tile();
        (x + local_x as WorldTileCoord, y + local_y as WorldTileCoord)
    }

    /// Returns the containing chunk when the absolute tile is in the
    /// representable chunk plane.
    pub fn from_tile(x: WorldTileCoord, y: WorldTileCoord) -> Option<Self> {
        let size = crate::simulation::CHUNK_SIZE as WorldTileCoord;
        Some(Self {
            x: i32::try_from(x.div_euclid(size)).ok()?,
            y: i32::try_from(y.div_euclid(size)).ok()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Chunk {
    pub coord: ChunkCoord,
    pub tiles: Vec<TileCell>,
    /// Derived from the terrain tile ids and the prototype catalog when the
    /// chunk is generated. Terrain mutation must update this cache if it is
    /// introduced in the future.
    #[serde(skip, default)]
    pub(crate) pollution_absorption_per_minute_milli: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TileCell {
    pub tile_id: TileId,
    pub collision: TileCollision,
    pub resource: Option<ResourceCell>,
}
