use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use factory_sim::{CHUNK_SIZE, Chunk, WorldSim};

use crate::constants::TILE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, tile_color, tile_hash};

pub(super) fn world_chunk_mesh(world: &WorldSim, chunk: &Chunk, ids: RenderPrototypeIds) -> Mesh {
    let size = CHUNK_SIZE as usize;
    let tile_colors = chunk
        .tiles
        .iter()
        .enumerate()
        .map(|(index, tile)| {
            let (x, y) = chunk
                .coord
                .tile_at((index % size) as i32, (index / size) as i32);
            tile_color(tile.tile_id, ids, world.seed, x, y)
                .to_linear()
                .to_f32_array()
        })
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

        let (world_x, world_y) = chunk.coord.tile_at(local_x as i32, local_y as i32);
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

    append_terrain_details(
        world,
        chunk,
        ids,
        &mut positions,
        &mut uvs,
        &mut colors,
        &mut indices,
    );

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

#[allow(clippy::too_many_arguments)]
fn append_terrain_details(
    world: &WorldSim,
    chunk: &Chunk,
    ids: RenderPrototypeIds,
    positions: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    let size = CHUNK_SIZE as usize;
    for (index, tile) in chunk.tiles.iter().enumerate() {
        let (x, y) = chunk
            .coord
            .tile_at((index % size) as i32, (index / size) as i32);
        let min_x = x as f32 * TILE_SIZE;
        let min_y = y as f32 * TILE_SIZE;
        let detail = tile_hash(world.seed, x, y, 0xa54f_f53a_5f1d_36f1);

        if ids.is_water(tile.tile_id) {
            append_water_foam(
                world, ids, x, y, min_x, min_y, positions, uvs, colors, indices,
            );
            continue;
        }

        // Sparse, tiny flecks read as grass blades or pebbles without turning
        // every tile into additional geometry.
        if detail.is_multiple_of(7) {
            let fleck_color = if ids.is_dirt(tile.tile_id) {
                Color::srgba(0.68, 0.56, 0.35, 0.34)
            } else {
                Color::srgba(0.48, 0.66, 0.30, 0.30)
            };
            let width = 0.55 + ((detail >> 8) & 3) as f32 * 0.18;
            let height = 0.34 + ((detail >> 10) & 3) as f32 * 0.13;
            let offset_x = 0.9 + ((detail >> 16) % 48) as f32 / 48.0 * 5.4;
            let offset_y = 0.9 + ((detail >> 24) % 48) as f32 / 48.0 * 5.4;
            append_quad(
                positions,
                uvs,
                colors,
                indices,
                [min_x + offset_x, min_y + offset_y],
                [min_x + offset_x + width, min_y + offset_y + height],
                0.02,
                fleck_color,
            );
        }

        // Occasional short cracks give broad areas a low-contrast seam layer.
        if detail % 13 == 1 {
            let horizontal = detail & 1 == 0;
            let length = 2.2 + ((detail >> 32) & 7) as f32 * 0.38;
            let offset = 1.1 + ((detail >> 40) % 40) as f32 / 40.0 * 4.8;
            let (min, max) = if horizontal {
                (
                    [min_x + 1.0, min_y + offset],
                    [min_x + 1.0 + length, min_y + offset + 0.22],
                )
            } else {
                (
                    [min_x + offset, min_y + 1.0],
                    [min_x + offset + 0.22, min_y + 1.0 + length],
                )
            };
            append_quad(
                positions,
                uvs,
                colors,
                indices,
                min,
                max,
                0.015,
                Color::srgba(0.08, 0.12, 0.075, 0.14),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn append_water_foam(
    world: &WorldSim,
    ids: RenderPrototypeIds,
    x: i64,
    y: i64,
    min_x: f32,
    min_y: f32,
    positions: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    const EDGES: [(i64, i64); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let foam = Color::srgba(0.55, 0.76, 0.82, 0.40);
    for (edge, (dx, dy)) in EDGES.into_iter().enumerate() {
        let Some(neighbor) = world.tile_at(x + dx, y + dy) else {
            continue;
        };
        if ids.is_water(neighbor.tile_id) {
            continue;
        }
        let edge_hash = tile_hash(world.seed, x, y, 0x510e_527f_ade6_82d1 + edge as u64);
        let wobble = 0.34 + (edge_hash % 4) as f32 * 0.08;
        let lead = 0.25 + ((edge_hash >> 8) % 5) as f32 * 0.12;
        let trail = 0.25 + ((edge_hash >> 12) % 5) as f32 * 0.12;
        let (min, max) = match edge {
            0 => (
                [min_x + lead, min_y],
                [min_x + TILE_SIZE - trail, min_y + wobble],
            ),
            1 => (
                [min_x + TILE_SIZE - wobble, min_y + lead],
                [min_x + TILE_SIZE, min_y + TILE_SIZE - trail],
            ),
            2 => (
                [min_x + lead, min_y + TILE_SIZE - wobble],
                [min_x + TILE_SIZE - trail, min_y + TILE_SIZE],
            ),
            _ => (
                [min_x, min_y + lead],
                [min_x + wobble, min_y + TILE_SIZE - trail],
            ),
        };
        append_quad(positions, uvs, colors, indices, min, max, 0.025, foam);
    }
}

#[allow(clippy::too_many_arguments)]
fn append_quad(
    positions: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    min: [f32; 2],
    max: [f32; 2],
    z: f32,
    color: Color,
) {
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        [min[0], min[1], z],
        [max[0], min[1], z],
        [max[0], max[1], z],
        [min[0], max[1], z],
    ]);
    uvs.extend_from_slice(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    colors.extend_from_slice(&[color.to_linear().to_f32_array(); 4]);
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
