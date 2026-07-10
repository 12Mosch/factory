use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};

use crate::map::resources::{MapDisplaySettings, MapTextureBounds};

pub fn map_texture_bounds(
    sim: &Simulation,
    settings: &MapDisplaySettings,
) -> Option<MapTextureBounds> {
    let chunks = if settings.debug_reveal_all {
        sim.world().chunks.keys().copied().collect::<Vec<_>>()
    } else {
        sim.revealed_chunks()
            .iter()
            .copied()
            .filter(|coord| sim.world().chunks.contains_key(coord))
            .collect::<Vec<_>>()
    };
    let (focus_x, focus_y) = sim.player().tile_position();
    chunk_texture_bounds_centered(chunks, (focus_x, focus_y))
}

#[allow(dead_code)]
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

fn chunk_texture_bounds_centered(
    chunk_coords: impl IntoIterator<Item = ChunkCoord>,
    focus: (i64, i64),
) -> Option<MapTextureBounds> {
    let mut coords = chunk_coords.into_iter();
    let first = coords.next()?;
    let (mut min_chunk_x, mut max_chunk_x) = (first.x, first.x);
    let (mut min_chunk_y, mut max_chunk_y) = (first.y, first.y);
    for coord in coords {
        min_chunk_x = min_chunk_x.min(coord.x);
        max_chunk_x = max_chunk_x.max(coord.x);
        min_chunk_y = min_chunk_y.min(coord.y);
        max_chunk_y = max_chunk_y.max(coord.y);
    }
    let origin = ChunkCoord {
        x: min_chunk_x,
        y: min_chunk_y,
    }
    .min_tile();
    let full_width = (i64::from(max_chunk_x) - i64::from(min_chunk_x) + 1) * i64::from(CHUNK_SIZE);
    let full_height = (i64::from(max_chunk_y) - i64::from(min_chunk_y) + 1) * i64::from(CHUNK_SIZE);
    let width = u32::try_from(full_width)
        .unwrap_or(u32::MAX)
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES);
    let height = u32::try_from(full_height)
        .unwrap_or(u32::MAX)
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES);
    let max_min_x = origin.0 + full_width - i64::from(width);
    let max_min_y = origin.1 + full_height - i64::from(height);
    let min_x = (focus.0 - i64::from(width) / 2).clamp(origin.0, max_min_x);
    let min_y = (focus.1 - i64::from(height) / 2).clamp(origin.1, max_min_y);
    Some(MapTextureBounds {
        min_x,
        min_y,
        width,
        height,
    })
}
