use crate::error::PrototypeLoadError;
use crate::ids::TileId;
use crate::model::TilePrototype;
use crate::raw::RawTilePrototype;
use crate::validation::resolve_collision_mask;

pub(super) fn load_tiles(
    tiles: Vec<RawTilePrototype>,
) -> Result<Vec<TilePrototype>, PrototypeLoadError> {
    tiles
        .into_iter()
        .map(|tile| {
            let name = tile.name;
            if tile.walking_speed_percent == 0 {
                return Err(PrototypeLoadError::InvalidTileMetadata {
                    tile: name,
                    detail: "walking_speed_percent must be at least 1",
                });
            }
            Ok(TilePrototype {
                id: TileId::new(tile.id),
                name: name.clone(),
                collision_mask: resolve_collision_mask(name, tile.collision_mask)?,
                pollution_absorption_per_minute_milli: tile.pollution_absorption_per_minute_milli,
                color: tile.color,
                walking_speed_percent: tile.walking_speed_percent,
            })
        })
        .collect()
}
