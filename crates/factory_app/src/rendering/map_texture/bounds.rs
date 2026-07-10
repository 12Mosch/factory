use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};

use crate::map::resources::{MapDisplaySettings, MapTextureBounds};

pub fn map_texture_bounds(
    sim: &Simulation,
    settings: &MapDisplaySettings,
) -> Option<MapTextureBounds> {
    if settings.debug_reveal_all {
        chunk_texture_bounds(sim.world().chunks.keys().copied())
    } else {
        chunk_texture_bounds(
            sim.revealed_chunks()
                .iter()
                .copied()
                .filter(|coord| sim.world().chunks.contains_key(coord)),
        )
    }
}

pub(super) fn chunk_texture_bounds(
    chunk_coords: impl IntoIterator<Item = ChunkCoord>,
) -> Option<MapTextureBounds> {
    let mut chunk_coords = chunk_coords.into_iter();
    let first = chunk_coords.next()?;
    let mut min_chunk_x = first.x;
    let mut max_chunk_x = first.x;
    let mut min_chunk_y = first.y;
    let mut max_chunk_y = first.y;

    for coord in chunk_coords {
        min_chunk_x = min_chunk_x.min(coord.x);
        max_chunk_x = max_chunk_x.max(coord.x);
        min_chunk_y = min_chunk_y.min(coord.y);
        max_chunk_y = max_chunk_y.max(coord.y);
    }

    Some(MapTextureBounds {
        min_x: ChunkCoord {
            x: min_chunk_x,
            y: min_chunk_y,
        }
        .min_tile()
        .0,
        min_y: ChunkCoord {
            x: min_chunk_x,
            y: min_chunk_y,
        }
        .min_tile()
        .1,
        width: u32::try_from(
            (i64::from(max_chunk_x) - i64::from(min_chunk_x) + 1) * i64::from(CHUNK_SIZE),
        )
        .ok()?
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES),
        height: u32::try_from(
            (i64::from(max_chunk_y) - i64::from(min_chunk_y) + 1) * i64::from(CHUNK_SIZE),
        )
        .ok()?
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES),
    })
}
