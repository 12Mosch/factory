use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use factory_sim::CHUNK_SIZE;

use crate::constants::TILE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, tile_color};

pub(super) fn world_chunk_mesh(chunk: &factory_sim::Chunk, ids: RenderPrototypeIds) -> Mesh {
    let size = CHUNK_SIZE as usize;
    let tile_colors = chunk
        .tiles
        .iter()
        .map(|tile| tile_color(tile.tile_id, ids).to_linear().to_f32_array())
        .collect::<Vec<_>>();

    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    // Greedy meshing: merge runs of same-colored tiles into maximal rectangles
    // so mostly-uniform terrain emits a few quads instead of one per tile.
    let mut merged = vec![false; chunk.tiles.len()];
    for start in 0..chunk.tiles.len() {
        if merged[start] {
            continue;
        }
        let color = tile_colors[start];
        let local_x = start % size;
        let local_y = start / size;

        let mut width = 1;
        while local_x + width < size
            && !merged[start + width]
            && tile_colors[start + width] == color
        {
            width += 1;
        }

        let mut height = 1;
        'grow: while local_y + height < size {
            let row = start + height * size;
            for dx in 0..width {
                if merged[row + dx] || tile_colors[row + dx] != color {
                    break 'grow;
                }
            }
            height += 1;
        }

        for dy in 0..height {
            merged[start + dy * size..start + dy * size + width].fill(true);
        }

        let world_x = chunk.coord.x * CHUNK_SIZE + local_x as i32;
        let world_y = chunk.coord.y * CHUNK_SIZE + local_y as i32;
        let min_x = world_x as f32 * TILE_SIZE;
        let min_y = world_y as f32 * TILE_SIZE;
        let max_x = min_x + width as f32 * TILE_SIZE;
        let max_y = min_y + height as f32 * TILE_SIZE;
        let base_index = positions.len() as u32;

        positions.extend_from_slice(&[
            [min_x, min_y, 0.0],
            [max_x, min_y, 0.0],
            [max_x, max_y, 0.0],
            [min_x, max_y, 0.0],
        ]);
        uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        colors.extend_from_slice(&[color; 4]);
        indices.extend_from_slice(&[
            base_index,
            base_index + 1,
            base_index + 2,
            base_index,
            base_index + 2,
            base_index + 3,
        ]);
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}
