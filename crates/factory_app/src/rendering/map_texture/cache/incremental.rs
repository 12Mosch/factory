use crate::map::resources::{MapLayerTextureCache, MapTextureLayer};

use super::super::bounds::map_texture_bounds;
use super::super::grid::draw_chunk_grid;
use super::super::rasterizer::{MapRasterizer, chunk_intersects_bounds};
use super::changes::MapTextureChanges;
use super::pixels::{add_newly_exposed_chunks, resize_cached_pixels};
use super::repaint::{repaint_all_chunks, repaint_dirty_chunks, repaint_dirty_tiles};

pub(super) fn update_map_pixels_incremental(
    rasterizer: &MapRasterizer<'_>,
    cache: &mut MapLayerTextureCache,
    map_changed: bool,
) {
    let old_bounds = cache.bounds.unwrap_or_default();
    let MapTextureChanges {
        mut dirty_chunks,
        dirty_resource_tiles,
        dirty_terrain_tiles,
    } = MapTextureChanges::collect(rasterizer, cache, map_changed);
    let new_bounds = if map_changed {
        map_texture_bounds(rasterizer.sim, rasterizer.settings).unwrap_or_default()
    } else {
        old_bounds
    };
    let bounds_changed = old_bounds != new_bounds;
    if bounds_changed {
        let background = if rasterizer.layer == MapTextureLayer::Surface {
            super::super::UNREVEALED_PIXEL
        } else {
            [0; 4]
        };
        resize_cached_pixels(cache, old_bounds, new_bounds, background);
        cache.dirty_regions.mark_full();
        // The resize only preserves pixels inside both bounds; drop the paint
        // state of clipped chunks so they get repainted at their new position.
        cache
            .painted_chunks
            .retain(|coord, _| chunk_intersects_bounds(*coord, new_bounds));
        if let Some(dirty_chunks) = dirty_chunks.as_mut() {
            add_newly_exposed_chunks(old_bounds, new_bounds, dirty_chunks);
            for coord in dirty_chunks.iter() {
                cache.painted_chunks.remove(coord);
            }
        }
    }

    if dirty_chunks.is_none() || dirty_resource_tiles.is_none() || dirty_terrain_tiles.is_none() {
        repaint_all_chunks(rasterizer, cache);
    } else {
        repaint_dirty_chunks(
            rasterizer,
            cache,
            dirty_chunks
                .as_deref()
                .expect("checked exact dirty chunk history"),
        );
        if rasterizer.layer == MapTextureLayer::Resources {
            repaint_dirty_tiles(
                rasterizer,
                cache,
                dirty_resource_tiles
                    .as_deref()
                    .expect("checked exact resource history")
                    .iter()
                    .map(|change| (change.x, change.y)),
            );
        }
        if rasterizer.layer == MapTextureLayer::Surface {
            repaint_dirty_tiles(
                rasterizer,
                cache,
                dirty_terrain_tiles
                    .as_deref()
                    .expect("checked exact terrain history")
                    .iter()
                    .map(|change| (change.x, change.y)),
            );
        }
    }

    if bounds_changed
        && rasterizer.settings.show_chunk_grid
        && rasterizer.layer == MapTextureLayer::Surface
    {
        let Some(bounds) = cache.bounds else {
            return;
        };
        let Some(data) = cache.pixels.as_mut() else {
            return;
        };
        draw_chunk_grid(data, bounds);
    }
}
