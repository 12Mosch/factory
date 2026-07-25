use crate::world::WorldTileCoord;
use factory_data::TileId;

/// One terrain tile rewritten after generation, in the same bounded-history
/// shape as [`crate::world::ResourceTileChange`]. Deferred consumers
/// (terrain meshes, the map texture) replay these to repaint exactly the
/// tiles that changed instead of rebuilding whole chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainTileChange {
    pub revision: u64,
    pub x: WorldTileCoord,
    pub y: WorldTileCoord,
    pub tile_id: TileId,
}

/// Why a terrain write was rejected. Placement consumes an item only after
/// the write succeeds, so every rejection leaves the world untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainMutationError {
    UnknownTile(TileId),
    OutsideGeneratedChunks {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
    /// The target already carries the requested tile.
    Unchanged {
        x: WorldTileCoord,
        y: WorldTileCoord,
    },
}
