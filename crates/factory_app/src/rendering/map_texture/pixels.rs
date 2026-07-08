use crate::map::resources::MapTextureBounds;

pub const UNREVEALED_PIXEL: [u8; 4] = [6, 7, 8, 255];
pub const GRID_PIXEL: [u8; 4] = [188, 139, 54, 255];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapPixels {
    pub bounds: MapTextureBounds,
    pub data: Vec<u8>,
}

pub(super) fn set_world_pixel(
    data: &mut [u8],
    bounds: MapTextureBounds,
    x: i32,
    y: i32,
    pixel: [u8; 4],
) {
    let local_x = x - bounds.min_x;
    let local_y = y - bounds.min_y;
    if local_x < 0 || local_y < 0 {
        return;
    }
    let local_x = local_x as u32;
    let local_y = local_y as u32;
    if local_x >= bounds.width || local_y >= bounds.height {
        return;
    }
    let flipped_y = bounds.height - 1 - local_y;
    let offset = ((flipped_y * bounds.width + local_x) * 4) as usize;
    data[offset..offset + 4].copy_from_slice(&pixel);
}

pub(super) fn pixel_offset(bounds: MapTextureBounds, x: i32, y: i32) -> usize {
    let local_x = (x - bounds.min_x) as u32;
    let local_y = (y - bounds.min_y) as u32;
    let flipped_y = bounds.height - 1 - local_y;
    ((flipped_y * bounds.width + local_x) * 4) as usize
}
