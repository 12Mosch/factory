use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation};

use crate::map::resources::{MapDisplaySettings, MapTextureBounds};

const MAP_BOUNDS_HYSTERESIS_CHUNKS: i64 = 8;

#[derive(Clone, Copy)]
struct ChunkExtents {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
}

impl ChunkExtents {
    fn from_chunks(chunk_coords: impl IntoIterator<Item = ChunkCoord>) -> Option<Self> {
        let mut chunk_coords = chunk_coords.into_iter();
        let first = chunk_coords.next()?;
        let mut extents = Self {
            min_x: first.x,
            max_x: first.x,
            min_y: first.y,
            max_y: first.y,
        };
        for coord in chunk_coords {
            extents.min_x = extents.min_x.min(coord.x);
            extents.max_x = extents.max_x.max(coord.x);
            extents.min_y = extents.min_y.min(coord.y);
            extents.max_y = extents.max_y.max(coord.y);
        }
        Some(extents)
    }
}

fn visible_chunk_extents(sim: &Simulation, settings: &MapDisplaySettings) -> Option<ChunkExtents> {
    if settings.debug_reveal_all {
        ChunkExtents::from_chunks(sim.world().chunks.keys().copied())
    } else {
        ChunkExtents::from_chunks(
            sim.revealed_chunks()
                .iter()
                .copied()
                .filter(|coord| sim.world().chunks.contains_key(coord)),
        )
    }
}

pub fn map_texture_bounds(
    sim: &Simulation,
    settings: &MapDisplaySettings,
) -> Option<MapTextureBounds> {
    let extents = visible_chunk_extents(sim, settings)?;
    let (focus_x, focus_y) = sim.player().tile_position();
    Some(chunk_texture_bounds_centered(extents, (focus_x, focus_y)))
}

/// Chooses storage bounds while retaining the current capped allocation until
/// the player leaves an inner hysteresis window. This turns per-tile movement
/// into infrequent, chunk-aligned shifts without broadening the map's logical
/// bounds to include unrevealed chunks.
pub(super) fn map_texture_bounds_with_hysteresis(
    sim: &Simulation,
    settings: &MapDisplaySettings,
    previous: MapTextureBounds,
) -> Option<MapTextureBounds> {
    let extents = visible_chunk_extents(sim, settings)?;
    let (focus_x, focus_y) = sim.player().tile_position();
    let candidate = chunk_texture_bounds_centered(extents, (focus_x, focus_y));
    if previous.width == 0 || previous.height == 0 {
        return Some(candidate);
    }

    let chunk_size = i64::from(CHUNK_SIZE);
    let x = stabilized_axis(
        previous.min_x,
        previous.width,
        candidate.min_x,
        candidate.width,
        i64::from(extents.min_x) * chunk_size,
        (i64::from(extents.max_x) + 1) * chunk_size,
        focus_x,
    );
    let y = stabilized_axis(
        previous.min_y,
        previous.height,
        candidate.min_y,
        candidate.height,
        i64::from(extents.min_y) * chunk_size,
        (i64::from(extents.max_y) + 1) * chunk_size,
        focus_y,
    );
    Some(MapTextureBounds {
        min_x: x.0,
        width: x.1,
        min_y: y.0,
        height: y.1,
    })
}

fn stabilized_axis(
    old_min: i64,
    old_len: u32,
    new_min: i64,
    new_len: u32,
    content_min: i64,
    content_max: i64,
    focus: i64,
) -> (i64, u32) {
    let chunk_size = i64::from(CHUNK_SIZE);
    let old_max = old_min + i64::from(old_len);
    let content_fits = content_min >= old_min && content_max <= old_max;
    if new_len <= old_len && content_fits {
        return (old_min, old_len);
    }

    let max_side = crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES;
    if old_len == max_side && new_len == max_side {
        let guard = MAP_BOUNDS_HYSTERESIS_CHUNKS * chunk_size;
        if focus >= old_min + guard && focus < old_max - guard {
            return (old_min, old_len);
        }
    }

    (new_min, new_len)
}

#[allow(dead_code)]
pub(super) fn chunk_texture_bounds(
    chunk_coords: impl IntoIterator<Item = ChunkCoord>,
) -> Option<MapTextureBounds> {
    let extents = ChunkExtents::from_chunks(chunk_coords)?;
    let chunk_size = i64::from(CHUNK_SIZE);

    Some(MapTextureBounds {
        min_x: i64::from(extents.min_x) * chunk_size,
        min_y: i64::from(extents.min_y) * chunk_size,
        width: u32::try_from(
            (i64::from(extents.max_x) - i64::from(extents.min_x) + 1) * chunk_size,
        )
        .ok()?
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES),
        height: u32::try_from(
            (i64::from(extents.max_y) - i64::from(extents.min_y) + 1) * chunk_size,
        )
        .ok()?
        .min(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES),
    })
}

fn chunk_texture_bounds_centered(extents: ChunkExtents, focus: (i64, i64)) -> MapTextureBounds {
    let (min_x, width) = centered_axis(extents.min_x, extents.max_x, focus.0);
    let (min_y, height) = centered_axis(extents.min_y, extents.max_y, focus.1);
    MapTextureBounds {
        min_x,
        min_y,
        width,
        height,
    }
}

fn centered_axis(min_chunk: i32, max_chunk: i32, focus: i64) -> (i64, u32) {
    let chunk_size = i64::from(CHUNK_SIZE);
    let max_chunks = i64::from(crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES) / chunk_size;
    let content_chunks = i64::from(max_chunk) - i64::from(min_chunk) + 1;
    let storage_chunks = content_chunks.min(max_chunks);
    let width = u32::try_from(storage_chunks * chunk_size)
        .expect("map texture dimensions are capped to u32");

    let padded_min_chunk = if content_chunks <= max_chunks {
        i64::from(min_chunk)
    } else {
        let focus_chunk = focus.div_euclid(chunk_size);
        let min_allowed = i64::from(min_chunk);
        let max_allowed = i64::from(max_chunk) - storage_chunks + 1;
        (focus_chunk - storage_chunks / 2).clamp(min_allowed, max_allowed)
    };
    (padded_min_chunk * chunk_size, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_axis_is_chunk_aligned() {
        let (min, width) = centered_axis(-2, 2, 0);

        assert_eq!(min.rem_euclid(i64::from(CHUNK_SIZE)), 0);
        assert_eq!(width % CHUNK_SIZE as u32, 0);
        assert_eq!(width, 5 * CHUNK_SIZE as u32);
    }

    #[test]
    fn capped_axis_waits_for_hysteresis_before_shifting() {
        let cap = crate::map::resources::MAX_MAP_TEXTURE_SIDE_TILES;
        let content = [-i64::from(CHUNK_SIZE), i64::from(cap)];

        assert_eq!(
            stabilized_axis(0, cap, 32, cap, content[0], content[1] + 32, 1024),
            (0, cap)
        );
        assert_eq!(
            stabilized_axis(0, cap, 32, cap, content[0], content[1] + 32, 1900),
            (32, cap)
        );
    }
}
