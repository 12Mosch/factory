use factory_sim::{CHUNK_SIZE, ChunkCoord};

use crate::map::resources::MapTextureBounds;

use super::pixels::{GRID_PIXEL, set_world_pixel};

fn next_chunk_boundary_at_or_after(value: i32) -> i32 {
    value.div_euclid(CHUNK_SIZE) * CHUNK_SIZE
        + if value.rem_euclid(CHUNK_SIZE) == 0 {
            0
        } else {
            CHUNK_SIZE
        }
}

pub(super) fn draw_chunk_grid(data: &mut [u8], bounds: MapTextureBounds) {
    let max_x = bounds.min_x + bounds.width as i32 - 1;
    let max_y = bounds.min_y + bounds.height as i32 - 1;

    let mut world_x = next_chunk_boundary_at_or_after(bounds.min_x);
    while world_x <= max_x {
        for world_y in bounds.min_y..=max_y {
            set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
        }
        world_x += CHUNK_SIZE;
    }

    let mut world_y = next_chunk_boundary_at_or_after(bounds.min_y);
    while world_y <= max_y {
        for world_x in bounds.min_x..=max_x {
            set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
        }
        world_y += CHUNK_SIZE;
    }
}

pub(super) fn draw_chunk_grid_for_chunk(
    data: &mut [u8],
    bounds: MapTextureBounds,
    coord: ChunkCoord,
) {
    let min_x = coord.x * CHUNK_SIZE;
    let min_y = coord.y * CHUNK_SIZE;
    for local_y in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            let world_x = min_x + local_x;
            let world_y = min_y + local_y;
            if world_x.rem_euclid(CHUNK_SIZE) == 0 || world_y.rem_euclid(CHUNK_SIZE) == 0 {
                set_world_pixel(data, bounds, world_x, world_y, GRID_PIXEL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::map_texture::pixels::pixel_offset;

    #[test]
    fn draw_chunk_grid_paints_exactly_the_chunk_boundary_pixels() {
        let bounds = MapTextureBounds {
            min_x: -CHUNK_SIZE * 2,
            min_y: -CHUNK_SIZE,
            width: (CHUNK_SIZE * 3) as u32,
            height: (CHUNK_SIZE * 2) as u32,
        };
        let mut data = vec![0; bounds.width as usize * bounds.height as usize * 4];

        draw_chunk_grid(&mut data, bounds);

        for world_y in bounds.min_y..bounds.min_y + bounds.height as i32 {
            for world_x in bounds.min_x..bounds.min_x + bounds.width as i32 {
                let offset = pixel_offset(bounds, world_x, world_y);
                let expected =
                    if world_x.rem_euclid(CHUNK_SIZE) == 0 || world_y.rem_euclid(CHUNK_SIZE) == 0 {
                        GRID_PIXEL
                    } else {
                        [0; 4]
                    };
                assert_eq!(
                    data[offset..offset + 4],
                    expected,
                    "pixel at ({world_x}, {world_y})"
                );
            }
        }
    }
}
