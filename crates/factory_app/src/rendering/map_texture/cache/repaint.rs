use factory_sim::ChunkCoord;

use crate::map::resources::MapLayerTextureCache;

use super::super::rasterizer::MapRasterizer;

pub(super) fn repaint_dirty_chunks(
    rasterizer: &MapRasterizer<'_>,
    cache: &mut MapLayerTextureCache,
    dirty_chunks: &[ChunkCoord],
) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    for &coord in dirty_chunks {
        if !rasterizer.chunk_is_eligible(coord, bounds) {
            continue;
        }
        let state = rasterizer.chunk_paint_state(coord);
        if cache.painted_chunks.get(&coord) == Some(&state) {
            continue;
        }
        rasterizer.repaint_chunk(data, bounds, coord);
        cache.dirty_regions.mark_world_chunk(bounds, coord);
        cache.painted_chunks.insert(coord, state);
    }
}

/// Repaints individual tiles that changed inside already-painted chunks.
/// Shared by the resource layer (mined-out cells) and the surface layer
/// (runtime terrain mutation).
pub(super) fn repaint_dirty_tiles(
    rasterizer: &MapRasterizer<'_>,
    cache: &mut MapLayerTextureCache,
    tiles: impl Iterator<Item = (i64, i64)>,
) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    for (x, y) in tiles.filter(|&(x, y)| {
        bounds.contains_tile((x, y))
            && ChunkCoord::from_tile(x, y)
                .is_some_and(|coord| rasterizer.chunk_paint_state(coord).revealed)
    }) {
        rasterizer.repaint_tile(data, bounds, x, y);
        cache.dirty_regions.mark_world_tile(bounds, x, y);
    }
}

pub(super) fn repaint_all_chunks(rasterizer: &MapRasterizer<'_>, cache: &mut MapLayerTextureCache) {
    let Some(bounds) = cache.bounds else {
        return;
    };
    let Some(data) = cache.pixels.as_mut() else {
        return;
    };

    cache.painted_chunks.clear();
    for coord in rasterizer.eligible_chunk_coords(bounds) {
        rasterizer.repaint_chunk(data, bounds, coord);
        cache
            .painted_chunks
            .insert(coord, rasterizer.chunk_paint_state(coord));
    }
    cache.dirty_regions.mark_full();
}

pub(super) fn refresh_painted_chunks(
    rasterizer: &MapRasterizer<'_>,
    cache: &mut MapLayerTextureCache,
) {
    let bounds = cache
        .bounds
        .expect("bounds must be set before refresh_painted_chunks");
    cache.painted_chunks = rasterizer
        .eligible_chunk_coords(bounds)
        .map(|coord| (coord, rasterizer.chunk_paint_state(coord)))
        .collect();
}
