use factory_sim::ChunkCoord;

use crate::map::resources::{MapLayerTextureCache, MapTextureBounds};

use super::super::pixels::pixel_offset;

pub(super) fn resize_cached_pixels(
    cache: &mut MapLayerTextureCache,
    old_bounds: MapTextureBounds,
    new_bounds: MapTextureBounds,
    background: [u8; 4],
) {
    let Some(old_pixels) = cache.pixels.take() else {
        cache.bounds = Some(new_bounds);
        cache.pixels =
            Some(background.repeat(new_bounds.width as usize * new_bounds.height as usize));
        cache.painted_chunks.clear();
        return;
    };

    if old_bounds.width == new_bounds.width && old_bounds.height == new_bounds.height {
        cache.pixels = Some(old_pixels);
        shift_cached_pixels_in_place(cache, old_bounds, new_bounds, background);
        cache.bounds = Some(new_bounds);
        return;
    }

    let mut new_pixels = background.repeat(new_bounds.width as usize * new_bounds.height as usize);
    let old_max_x = old_bounds.min_x + i64::from(old_bounds.width) - 1;
    let old_max_y = old_bounds.min_y + i64::from(old_bounds.height) - 1;
    let new_max_x = new_bounds.min_x + i64::from(new_bounds.width) - 1;
    let new_max_y = new_bounds.min_y + i64::from(new_bounds.height) - 1;
    let min_x = old_bounds.min_x.max(new_bounds.min_x);
    let max_x = old_max_x.min(new_max_x);
    let min_y = old_bounds.min_y.max(new_bounds.min_y);
    let max_y = old_max_y.min(new_max_y);

    if min_x <= max_x && min_y <= max_y {
        let row_len = (max_x - min_x + 1) as usize * 4;
        for world_y in min_y..=max_y {
            let old_offset = pixel_offset(old_bounds, min_x, world_y);
            let new_offset = pixel_offset(new_bounds, min_x, world_y);
            new_pixels[new_offset..new_offset + row_len]
                .copy_from_slice(&old_pixels[old_offset..old_offset + row_len]);
        }
    }

    cache.bounds = Some(new_bounds);
    cache.pixels = Some(new_pixels);
}

/// Shifts a same-sized capped texture without allocating a second 16 MiB
/// layer. `copy_within` order follows the source/destination offset so rows
/// that are still needed are never overwritten by an earlier move.
fn shift_cached_pixels_in_place(
    cache: &mut MapLayerTextureCache,
    old_bounds: MapTextureBounds,
    new_bounds: MapTextureBounds,
    background: [u8; 4],
) {
    let pixels = cache
        .pixels
        .as_mut()
        .expect("same-sized shift retains the pixel buffer");
    let old_max_x = old_bounds.min_x + i64::from(old_bounds.width);
    let old_max_y = old_bounds.min_y + i64::from(old_bounds.height);
    let new_max_x = new_bounds.min_x + i64::from(new_bounds.width);
    let new_max_y = new_bounds.min_y + i64::from(new_bounds.height);
    let min_x = old_bounds.min_x.max(new_bounds.min_x);
    let max_x = old_max_x.min(new_max_x);
    let min_y = old_bounds.min_y.max(new_bounds.min_y);
    let max_y = old_max_y.min(new_max_y);

    if min_x >= max_x || min_y >= max_y {
        pixels.as_mut_slice().fill_with({
            let mut channel = 0;
            move || {
                let value = background[channel];
                channel = (channel + 1) % 4;
                value
            }
        });
        return;
    }

    let row_len = (max_x - min_x) as usize * 4;
    let first_old = pixel_offset(old_bounds, min_x, min_y);
    let first_new = pixel_offset(new_bounds, min_x, min_y);
    let mut copy_row = |world_y| {
        let old_offset = pixel_offset(old_bounds, min_x, world_y);
        let new_offset = pixel_offset(new_bounds, min_x, world_y);
        pixels.copy_within(old_offset..old_offset + row_len, new_offset);
    };
    if first_new > first_old {
        // World Y is stored upside-down in the image, so ascending world rows
        // traverse descending buffer offsets.
        for world_y in min_y..max_y {
            copy_row(world_y);
        }
    } else {
        for world_y in (min_y..max_y).rev() {
            copy_row(world_y);
        }
    }

    fill_world_rect(
        pixels,
        new_bounds,
        new_bounds.min_x,
        new_bounds.min_y,
        new_max_x,
        min_y,
        background,
    );
    fill_world_rect(
        pixels,
        new_bounds,
        new_bounds.min_x,
        max_y,
        new_max_x,
        new_max_y,
        background,
    );
    fill_world_rect(
        pixels,
        new_bounds,
        new_bounds.min_x,
        min_y,
        min_x,
        max_y,
        background,
    );
    fill_world_rect(
        pixels, new_bounds, max_x, min_y, new_max_x, max_y, background,
    );
}

fn fill_world_rect(
    pixels: &mut [u8],
    bounds: MapTextureBounds,
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
    background: [u8; 4],
) {
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    let row_bytes = (max_x - min_x) as usize * 4;
    for world_y in min_y..max_y {
        let offset = pixel_offset(bounds, min_x, world_y);
        for pixel in pixels[offset..offset + row_bytes].as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&background);
        }
    }
}

pub(super) fn add_newly_exposed_chunks(
    old_bounds: MapTextureBounds,
    new_bounds: MapTextureBounds,
    dirty_chunks: &mut Vec<ChunkCoord>,
) {
    let new_max_x = new_bounds.min_x + i64::from(new_bounds.width);
    let new_max_y = new_bounds.min_y + i64::from(new_bounds.height);
    let overlap_min_x = new_bounds.min_x.max(old_bounds.min_x);
    let overlap_min_y = new_bounds.min_y.max(old_bounds.min_y);
    let overlap_max_x = new_max_x.min(old_bounds.min_x + i64::from(old_bounds.width));
    let overlap_max_y = new_max_y.min(old_bounds.min_y + i64::from(old_bounds.height));

    if overlap_min_x >= overlap_max_x || overlap_min_y >= overlap_max_y {
        add_chunks_intersecting_tile_rect(
            dirty_chunks,
            new_bounds.min_x,
            new_bounds.min_y,
            new_max_x,
            new_max_y,
        );
    } else {
        add_chunks_intersecting_tile_rect(
            dirty_chunks,
            new_bounds.min_x,
            new_bounds.min_y,
            new_max_x,
            overlap_min_y,
        );
        add_chunks_intersecting_tile_rect(
            dirty_chunks,
            new_bounds.min_x,
            overlap_max_y,
            new_max_x,
            new_max_y,
        );
        add_chunks_intersecting_tile_rect(
            dirty_chunks,
            new_bounds.min_x,
            overlap_min_y,
            overlap_min_x,
            overlap_max_y,
        );
        add_chunks_intersecting_tile_rect(
            dirty_chunks,
            overlap_max_x,
            overlap_min_y,
            new_max_x,
            overlap_max_y,
        );
    }

    dirty_chunks.sort_unstable();
    dirty_chunks.dedup();
}

fn add_chunks_intersecting_tile_rect(
    chunks: &mut Vec<ChunkCoord>,
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
) {
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    let (Some(min), Some(max)) = (
        ChunkCoord::from_tile(min_x, min_y),
        ChunkCoord::from_tile(max_x - 1, max_y - 1),
    ) else {
        return;
    };

    for y in min.y..=max.y {
        for x in min.x..=max.x {
            chunks.push(ChunkCoord { x, y });
        }
    }
}
