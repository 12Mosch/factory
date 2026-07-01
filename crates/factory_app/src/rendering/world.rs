use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};
use factory_sim::CHUNK_SIZE;

use crate::constants::TILE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, tile_color};
use crate::resources::{SimResource, VisibleChunks, WorldRenderCache};
use crate::save_load::PresentationReloadToken;

#[derive(Component)]
pub struct WorldChunkMesh;

pub(crate) fn sync_visible_world_tiles(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleChunks>,
    token: Res<PresentationReloadToken>,
    mut cache: ResMut<WorldRenderCache>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<ColorMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };

    if cache.last_reload_token != token.value {
        for (_, entity) in std::mem::take(&mut cache.chunk_entities) {
            commands.entity(entity).despawn();
        }
        cache.material = None;
        cache.last_reload_token = token.value;
    }

    let stale_chunks = cache
        .chunk_entities
        .keys()
        .copied()
        .filter(|coord| {
            !visible.chunks.contains(coord) || !sim.sim.world().chunks.contains_key(coord)
        })
        .collect::<Vec<_>>();
    for coord in stale_chunks {
        if let Some(entity) = cache.chunk_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
    }

    let ids = RenderPrototypeIds::from_catalog(sim.sim.catalog());
    let material = cache
        .material
        .get_or_insert_with(|| materials.add(ColorMaterial::from_color(Color::WHITE)))
        .clone();

    for coord in &visible.chunks {
        if cache.chunk_entities.contains_key(coord) {
            continue;
        }
        let Some(chunk) = sim.sim.world().chunks.get(coord) else {
            continue;
        };
        let entity = commands
            .spawn((
                Mesh2d(meshes.add(world_chunk_mesh(chunk, ids))),
                MeshMaterial2d(material.clone()),
                Transform::default(),
                WorldChunkMesh,
            ))
            .id();
        cache.chunk_entities.insert(*coord, entity);
    }

    cache.last_visible_revision = visible.revision;
    cache.last_chunk_revision = sim.sim.world().chunk_revision();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::belts::{
        BeltDirectionSprite, BeltItemSprite, sync_belt_direction_rendering,
        sync_belt_item_rendering,
    };
    use crate::rendering::entities::{PlacedEntitySprite, sync_placed_entity_rendering};
    use crate::rendering::resources::{
        ResourceRenderCache, ResourceRenderSettings, ResourceSprite, sync_resource_debug_rendering,
    };
    use crate::resources::{MapTextureBounds, RenderSyncStats};
    use factory_data::{BasePrototypeIds, entity_prototype_id_by_name};
    use factory_sim::{ChunkCoord, Direction, Simulation};
    use std::collections::BTreeSet;

    #[test]
    fn render_sync_counts_are_bounded_by_visible_chunks() {
        let mut sim = Simulation::new_test_world(123);
        for y in -10..10 {
            for x in -10..10 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
        place_belts_across_generated_world(&mut sim);

        let visible_chunks = BTreeSet::from([
            ChunkCoord { x: -1, y: -1 },
            ChunkCoord { x: 0, y: -1 },
            ChunkCoord { x: -1, y: 0 },
            ChunkCoord { x: 0, y: 0 },
        ]);
        let visible = VisibleChunks {
            chunks: visible_chunks,
            tile_bounds: Some(MapTextureBounds {
                min_x: -CHUNK_SIZE,
                min_y: -CHUNK_SIZE,
                width: (CHUNK_SIZE * 2) as u32,
                height: (CHUNK_SIZE * 2) as u32,
            }),
            revision: 1,
        };

        let total_generated_chunks = sim.world().generated_chunk_count();
        let total_entities = sim.entities().placed_len();
        let mut app = App::new();
        app.insert_resource(SimResource { sim })
            .insert_resource(visible)
            .init_resource::<WorldRenderCache>()
            .init_resource::<ResourceRenderCache>()
            .insert_resource(ResourceRenderSettings {
                show_amount_labels: false,
            })
            .init_resource::<RenderSyncStats>()
            .init_resource::<PresentationReloadToken>()
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default())
            .add_systems(
                Update,
                (
                    sync_visible_world_tiles,
                    sync_resource_debug_rendering,
                    sync_placed_entity_rendering,
                    sync_belt_direction_rendering,
                    sync_belt_item_rendering,
                )
                    .chain(),
            );

        app.update();

        let visible_chunk_count = app.world().resource::<VisibleChunks>().chunks.len();
        assert!(total_generated_chunks > visible_chunk_count);
        assert_eq!(
            app.world()
                .resource::<WorldRenderCache>()
                .chunk_entities
                .len(),
            visible_chunk_count
        );
        assert!(
            app.world()
                .resource::<ResourceRenderCache>()
                .sprite_entities
                .keys()
                .all(
                    |(x, y)| app
                        .world()
                        .resource::<VisibleChunks>()
                        .chunks
                        .contains(&ChunkCoord {
                            x: x.div_euclid(CHUNK_SIZE),
                            y: y.div_euclid(CHUNK_SIZE),
                        })
                )
        );
        assert!(total_entities > placed_entity_sprite_count(&mut app));
        assert!(placed_entity_sprite_count(&mut app) <= visible_chunk_count * CHUNK_SIZE as usize);
        assert!(belt_direction_sprite_count(&mut app) <= placed_entity_sprite_count(&mut app) * 2);
        assert!(belt_item_sprite_count(&mut app) <= placed_entity_sprite_count(&mut app));
        assert!(
            resource_sprite_count(&mut app)
                <= visible_chunk_count * (CHUNK_SIZE * CHUNK_SIZE) as usize
        );
    }

    fn place_belts_across_generated_world(sim: &mut Simulation) {
        let belt = entity_prototype_id_by_name(sim.catalog(), "transport_belt");
        let iron_ore = BasePrototypeIds::from_catalog(sim.catalog()).items.iron_ore;
        let coords = sim.world().chunks.keys().copied().collect::<Vec<_>>();

        for coord in coords {
            let Some((x, y)) = first_placeable_tile_in_chunk(sim, coord, belt) else {
                continue;
            };
            let entity_id = sim
                .place_entity(belt, x, y, Direction::East)
                .expect("validated belt should place");
            let _ = sim.insert_item_onto_belt(entity_id, 0, iron_ore);
        }
    }

    fn first_placeable_tile_in_chunk(
        sim: &Simulation,
        coord: ChunkCoord,
        prototype_id: factory_data::EntityPrototypeId,
    ) -> Option<(i32, i32)> {
        for y in coord.y * CHUNK_SIZE..(coord.y + 1) * CHUNK_SIZE {
            for x in coord.x * CHUNK_SIZE..(coord.x + 1) * CHUNK_SIZE {
                if sim
                    .can_place_entity(prototype_id, x, y, Direction::East)
                    .is_ok()
                {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn placed_entity_sprite_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<PlacedEntitySprite>>()
            .iter(app.world())
            .count()
    }

    fn belt_direction_sprite_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<BeltDirectionSprite>>()
            .iter(app.world())
            .count()
    }

    fn belt_item_sprite_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<BeltItemSprite>>()
            .iter(app.world())
            .count()
    }

    fn resource_sprite_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<ResourceSprite>>()
            .iter(app.world())
            .count()
    }
}
