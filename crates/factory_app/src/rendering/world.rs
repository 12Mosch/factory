use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};
use factory_sim::CHUNK_SIZE;

use crate::constants::TILE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, tile_color};
use crate::resources::SimResource;

pub(crate) fn spawn_world_tiles(
    mut commands: Commands,
    sim: Res<SimResource>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<ColorMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let ids = RenderPrototypeIds::from_catalog(sim.sim.catalog());
    let material = materials.add(ColorMaterial::from_color(Color::WHITE));

    for chunk in sim.sim.world().chunks.values() {
        commands.spawn((
            Mesh2d(meshes.add(world_chunk_mesh(chunk, ids))),
            MeshMaterial2d(material.clone()),
            Transform::default(),
        ));
    }
}

fn world_chunk_mesh(chunk: &factory_sim::Chunk, ids: RenderPrototypeIds) -> Mesh {
    let tile_count = chunk.tiles.len();
    let mut positions = Vec::with_capacity(tile_count * 4);
    let mut uvs = Vec::with_capacity(tile_count * 4);
    let mut colors = Vec::with_capacity(tile_count * 4);
    let mut indices = Vec::with_capacity(tile_count * 6);

    for (index, tile) in chunk.tiles.iter().enumerate() {
        let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
        let local_y = (index as i32).div_euclid(CHUNK_SIZE);
        let world_x = chunk.coord.x * CHUNK_SIZE + local_x;
        let world_y = chunk.coord.y * CHUNK_SIZE + local_y;
        let min_x = world_x as f32 * TILE_SIZE;
        let min_y = world_y as f32 * TILE_SIZE;
        let max_x = min_x + TILE_SIZE;
        let max_y = min_y + TILE_SIZE;
        let color = tile_color(tile.tile_id, ids).to_linear().to_f32_array();
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
