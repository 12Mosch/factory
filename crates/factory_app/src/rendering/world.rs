use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};
use factory_sim::CHUNK_SIZE;

use crate::constants::TILE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, tile_color};
use crate::map::resources::VisibleChunks;
use crate::rendering::resources::{RenderSyncStats, WorldRenderCache};
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;
use std::time::Instant;

#[derive(Component)]
pub struct WorldChunkMesh;

pub(crate) fn measured_sync_visible_world_tiles(
    commands: Commands,
    params: WorldTilesRenderParams,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_visible_world_tiles(commands, params);
    stats.record_world_tiles(started.elapsed());
}

pub(crate) fn sync_visible_world_tiles(mut commands: Commands, params: WorldTilesRenderParams) {
    sync_visible_world_tiles_impl(&mut commands, params);
}

#[derive(SystemParam)]
pub(crate) struct WorldTilesRenderParams<'w> {
    sim: Res<'w, SimResource>,
    visible: Res<'w, VisibleChunks>,
    token: Res<'w, PresentationReloadToken>,
    cache: ResMut<'w, WorldRenderCache>,
    meshes: Option<ResMut<'w, Assets<Mesh>>>,
    materials: Option<ResMut<'w, Assets<ColorMaterial>>>,
}

fn sync_visible_world_tiles_impl(commands: &mut Commands, params: WorldTilesRenderParams) {
    let WorldTilesRenderParams {
        sim,
        visible,
        token,
        mut cache,
        meshes,
        materials,
    } = params;
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };

    if cache.last_reload_token == token.value
        && cache.last_visible_revision == visible.revision
        && cache.last_chunk_revision == sim.sim.world().chunk_revision()
    {
        return;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendering::belts::{
        BeltDirectionSprite, BeltItemSprite, measured_sync_belt_direction_rendering,
        measured_sync_belt_item_rendering,
    };
    use crate::rendering::entities::{
        PlacedEntitySprite, measured_sync_placed_entity_rendering, update_visible_entity_ids,
    };
    use crate::map::resources::MapTextureBounds;
    use crate::rendering::resource_cells::{
        ResourceAmountLabel, ResourceRenderCache, ResourceRenderSettings, ResourceSprite,
        measured_sync_resource_debug_rendering, sync_resource_debug_rendering,
    };
    use crate::rendering::resources::{
        BeltItemRenderPool, RenderDetail, RenderSyncStats, VisibleEntityIds,
    };
    use factory_data::{BasePrototypeIds, entity_prototype_id_by_name, item_id_by_name};
    use factory_sim::{CHUNK_SIZE, ChunkCoord, Direction, Simulation};
    use std::collections::BTreeSet;
    use std::time::Duration;

    const RENDER_SYNC_SMALL_MEASUREMENT_FRAMES: usize = 300;
    const RENDER_SYNC_SMALL_TOTAL_P95_BUDGET: Duration = Duration::from_millis(4);
    const RENDER_SYNC_SMALL_TOTAL_MAX_BUDGET: Duration = Duration::from_millis(8);

    #[test]
    fn world_chunk_mesh_merges_tiles_without_changing_coverage() {
        let mut sim = Simulation::new_test_world(123);
        sim.ensure_chunk_generated(ChunkCoord { x: 0, y: 0 });
        let chunk = sim.world().chunks[&ChunkCoord { x: 0, y: 0 }].clone();
        let ids = RenderPrototypeIds::from_catalog(sim.catalog());

        let mesh = world_chunk_mesh(&chunk, ids);
        let (positions, colors) = mesh_quads(&mesh);
        let quad_count = positions.len() / 4;
        let tile_count = chunk.tiles.len();
        assert!(
            quad_count < tile_count,
            "greedy meshing should emit fewer quads ({quad_count}) than tiles ({tile_count})"
        );

        let size = CHUNK_SIZE as usize;
        let mut painted = vec![None; tile_count];
        for quad in 0..quad_count {
            let min = positions[quad * 4];
            let max = positions[quad * 4 + 2];
            let color = colors[quad * 4];
            let min_tile_x = (min[0] / TILE_SIZE).round() as i32 - chunk.coord.x * CHUNK_SIZE;
            let min_tile_y = (min[1] / TILE_SIZE).round() as i32 - chunk.coord.y * CHUNK_SIZE;
            let max_tile_x = (max[0] / TILE_SIZE).round() as i32 - chunk.coord.x * CHUNK_SIZE;
            let max_tile_y = (max[1] / TILE_SIZE).round() as i32 - chunk.coord.y * CHUNK_SIZE;
            for tile_y in min_tile_y..max_tile_y {
                for tile_x in min_tile_x..max_tile_x {
                    let index = tile_y as usize * size + tile_x as usize;
                    assert!(painted[index].is_none(), "quads must not overlap");
                    painted[index] = Some(color);
                }
            }
        }

        for (index, tile) in chunk.tiles.iter().enumerate() {
            let expected = tile_color(tile.tile_id, ids).to_linear().to_f32_array();
            assert_eq!(painted[index], Some(expected), "tile {index} coverage");
        }
    }

    #[test]
    fn world_chunk_mesh_collapses_uniform_chunk_to_single_quad() {
        let mut sim = Simulation::new_test_world(123);
        sim.ensure_chunk_generated(ChunkCoord { x: 0, y: 0 });
        let mut chunk = sim.world().chunks[&ChunkCoord { x: 0, y: 0 }].clone();
        let uniform_id = chunk.tiles[0].tile_id;
        for tile in &mut chunk.tiles {
            tile.tile_id = uniform_id;
        }
        let ids = RenderPrototypeIds::from_catalog(sim.catalog());

        let mesh = world_chunk_mesh(&chunk, ids);
        let (positions, _) = mesh_quads(&mesh);
        assert_eq!(positions.len(), 4);
    }

    fn mesh_quads(mesh: &Mesh) -> (Vec<[f32; 3]>, Vec<[f32; 4]>) {
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("world chunk mesh should have Float32x3 positions");
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("world chunk mesh should have Float32x4 colors");
        };
        (positions.clone(), colors.clone())
    }

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
            .init_resource::<VisibleEntityIds>()
            .init_resource::<RenderDetail>()
            .init_resource::<BeltItemRenderPool>()
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
                    measured_sync_visible_world_tiles,
                    measured_sync_resource_debug_rendering,
                    update_visible_entity_ids,
                    measured_sync_placed_entity_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
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

    #[test]
    fn resource_visibility_changes_reuse_overlapping_sprites_and_labels() {
        let mut sim = Simulation::new_test_world(123);
        for y in -10..=10 {
            for x in -10..=10 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }

        let (resource_chunk, resource_coord) = sim
            .world()
            .chunks
            .iter()
            .find_map(|(&coord, chunk)| {
                resource_coord_in_chunk(coord, chunk).map(|tile| (coord, tile))
            })
            .expect("generated test world should include a resource chunk");
        let first_visible = visible_for_chunks([
            ChunkCoord {
                x: resource_chunk.x - 1,
                y: resource_chunk.y,
            },
            resource_chunk,
        ]);
        let mut second_visible = visible_for_chunks([
            resource_chunk,
            ChunkCoord {
                x: resource_chunk.x + 1,
                y: resource_chunk.y,
            },
        ]);
        second_visible.revision = 2;

        let mut app = App::new();
        app.insert_resource(SimResource { sim })
            .insert_resource(first_visible)
            .init_resource::<ResourceRenderCache>()
            .insert_resource(ResourceRenderSettings {
                show_amount_labels: true,
            })
            .init_resource::<RenderDetail>()
            .add_systems(Update, sync_resource_debug_rendering);

        app.update();
        let first_sprite = app
            .world()
            .resource::<ResourceRenderCache>()
            .sprite_entities[&resource_coord];
        let first_label =
            app.world().resource::<ResourceRenderCache>().label_entities[&resource_coord];

        *app.world_mut().resource_mut::<VisibleChunks>() = second_visible;
        app.update();

        let cache = app.world().resource::<ResourceRenderCache>();
        assert_eq!(cache.sprite_entities[&resource_coord], first_sprite);
        assert_eq!(cache.label_entities[&resource_coord], first_label);
        let sprite_entities = cache.sprite_entities.len();
        let label_entities = cache.label_entities.len();
        assert_eq!(resource_sprite_count(&mut app), sprite_entities);
        assert_eq!(resource_label_count(&mut app), label_entities);
    }

    #[test]
    #[ignore]
    fn render_sync_small_visual_load_budget() {
        let sim = small_render_sync_fixture();
        let visible = visible_window();
        let mut app = render_sync_app(sim, visible);

        app.update();
        let stats =
            collect_render_sync_budget_stats(&mut app, RENDER_SYNC_SMALL_MEASUREMENT_FRAMES);
        print_render_sync_budget_stats(&mut app, stats);

        assert!(
            stats.p95.total <= RENDER_SYNC_SMALL_TOTAL_P95_BUDGET,
            "render sync total p95 {:.3} ms exceeded budget {:.3} ms",
            ms(stats.p95.total),
            ms(RENDER_SYNC_SMALL_TOTAL_P95_BUDGET)
        );
        assert!(
            stats.max.total <= RENDER_SYNC_SMALL_TOTAL_MAX_BUDGET,
            "render sync total max {:.3} ms exceeded budget {:.3} ms",
            ms(stats.max.total),
            ms(RENDER_SYNC_SMALL_TOTAL_MAX_BUDGET)
        );
    }

    fn small_render_sync_fixture() -> Simulation {
        let mut sim = Simulation::new_test_world(123);
        for y in -4..=4 {
            for x in -4..=4 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }

        place_entities(&mut sim, "assembling_machine", 100, Direction::North);
        let belts = place_entities(&mut sim, "transport_belt", 1_000, Direction::East);
        let iron_ore = item_id_by_name(sim.catalog(), "iron_ore");
        for belt_id in belts {
            let _ = sim.insert_item_onto_belt(belt_id, 0, iron_ore);
            let _ = sim.insert_item_onto_belt(belt_id, 1, iron_ore);
        }

        sim
    }

    fn visible_window() -> VisibleChunks {
        let visible_chunks = (-4..=-2)
            .flat_map(|y| (-4..=-2).map(move |x| ChunkCoord { x, y }))
            .collect::<BTreeSet<_>>();
        VisibleChunks {
            chunks: visible_chunks,
            tile_bounds: Some(MapTextureBounds {
                min_x: -4 * CHUNK_SIZE,
                min_y: -4 * CHUNK_SIZE,
                width: (CHUNK_SIZE * 3) as u32,
                height: (CHUNK_SIZE * 3) as u32,
            }),
            revision: 1,
        }
    }

    fn visible_for_chunks<const N: usize>(chunks: [ChunkCoord; N]) -> VisibleChunks {
        let chunks = BTreeSet::from(chunks);
        let min_chunk_x = chunks
            .iter()
            .map(|coord| coord.x)
            .min()
            .expect("visible chunks should not be empty");
        let max_chunk_x = chunks
            .iter()
            .map(|coord| coord.x)
            .max()
            .expect("visible chunks should not be empty");
        let min_chunk_y = chunks
            .iter()
            .map(|coord| coord.y)
            .min()
            .expect("visible chunks should not be empty");
        let max_chunk_y = chunks
            .iter()
            .map(|coord| coord.y)
            .max()
            .expect("visible chunks should not be empty");

        VisibleChunks {
            chunks,
            tile_bounds: Some(MapTextureBounds {
                min_x: min_chunk_x * CHUNK_SIZE,
                min_y: min_chunk_y * CHUNK_SIZE,
                width: ((max_chunk_x - min_chunk_x + 1) * CHUNK_SIZE) as u32,
                height: ((max_chunk_y - min_chunk_y + 1) * CHUNK_SIZE) as u32,
            }),
            revision: 1,
        }
    }

    fn resource_coord_in_chunk(
        coord: ChunkCoord,
        chunk: &factory_sim::Chunk,
    ) -> Option<(i32, i32)> {
        chunk.tiles.iter().enumerate().find_map(|(index, tile)| {
            tile.resource.map(|_| {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                (
                    coord.x * CHUNK_SIZE + local_x,
                    coord.y * CHUNK_SIZE + local_y,
                )
            })
        })
    }

    fn render_sync_app(sim: Simulation, visible: VisibleChunks) -> App {
        let mut app = App::new();
        app.insert_resource(SimResource { sim })
            .insert_resource(visible)
            .init_resource::<WorldRenderCache>()
            .init_resource::<ResourceRenderCache>()
            .init_resource::<VisibleEntityIds>()
            .init_resource::<RenderDetail>()
            .init_resource::<BeltItemRenderPool>()
            .insert_resource(ResourceRenderSettings {
                show_amount_labels: true,
            })
            .init_resource::<RenderSyncStats>()
            .init_resource::<PresentationReloadToken>()
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<ColorMaterial>::default())
            .add_systems(
                Update,
                (
                    measured_sync_visible_world_tiles,
                    measured_sync_resource_debug_rendering,
                    update_visible_entity_ids,
                    measured_sync_placed_entity_rendering,
                    measured_sync_belt_direction_rendering,
                    measured_sync_belt_item_rendering,
                )
                    .chain(),
            );
        app
    }

    #[derive(Clone, Copy)]
    struct RenderSyncSample {
        stats: RenderSyncStats,
    }

    #[derive(Clone, Copy)]
    struct RenderSyncBudgetStats {
        average: RenderSyncStats,
        p95: RenderSyncStats,
        max: RenderSyncStats,
    }

    fn collect_render_sync_budget_stats(app: &mut App, frames: usize) -> RenderSyncBudgetStats {
        assert!(frames > 0);
        let mut samples = Vec::with_capacity(frames);

        for _ in 0..frames {
            app.update();
            samples.push(RenderSyncSample {
                stats: *app.world().resource::<RenderSyncStats>(),
            });
        }

        render_sync_budget_stats(samples.into_iter().map(|sample| sample.stats).collect())
    }

    fn render_sync_budget_stats(mut samples: Vec<RenderSyncStats>) -> RenderSyncBudgetStats {
        assert!(!samples.is_empty());
        samples.sort_by_key(|stats| stats.total);
        let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);

        RenderSyncBudgetStats {
            average: average_render_sync_stats(&samples),
            p95: percentile_render_sync_stats(&samples, p95_index),
            max: max_render_sync_stats(&samples),
        }
    }

    fn average_render_sync_stats(samples: &[RenderSyncStats]) -> RenderSyncStats {
        RenderSyncStats {
            player: average_duration(samples, |stats| stats.player),
            world_tiles: average_duration(samples, |stats| stats.world_tiles),
            resources: average_duration(samples, |stats| stats.resources),
            placed_entities: average_duration(samples, |stats| stats.placed_entities),
            belt_directions: average_duration(samples, |stats| stats.belt_directions),
            belt_items: average_duration(samples, |stats| stats.belt_items),
            total: average_duration(samples, |stats| stats.total),
        }
    }

    fn percentile_render_sync_stats(samples: &[RenderSyncStats], index: usize) -> RenderSyncStats {
        RenderSyncStats {
            player: percentile_duration(samples, index, |stats| stats.player),
            world_tiles: percentile_duration(samples, index, |stats| stats.world_tiles),
            resources: percentile_duration(samples, index, |stats| stats.resources),
            placed_entities: percentile_duration(samples, index, |stats| stats.placed_entities),
            belt_directions: percentile_duration(samples, index, |stats| stats.belt_directions),
            belt_items: percentile_duration(samples, index, |stats| stats.belt_items),
            total: percentile_duration(samples, index, |stats| stats.total),
        }
    }

    fn max_render_sync_stats(samples: &[RenderSyncStats]) -> RenderSyncStats {
        RenderSyncStats {
            player: max_duration(samples, |stats| stats.player),
            world_tiles: max_duration(samples, |stats| stats.world_tiles),
            resources: max_duration(samples, |stats| stats.resources),
            placed_entities: max_duration(samples, |stats| stats.placed_entities),
            belt_directions: max_duration(samples, |stats| stats.belt_directions),
            belt_items: max_duration(samples, |stats| stats.belt_items),
            total: max_duration(samples, |stats| stats.total),
        }
    }

    fn average_duration(
        samples: &[RenderSyncStats],
        duration: impl Fn(RenderSyncStats) -> Duration,
    ) -> Duration {
        let nanos = samples
            .iter()
            .map(|sample| duration(*sample).as_nanos())
            .sum::<u128>()
            / samples.len() as u128;
        Duration::from_nanos(nanos as u64)
    }

    fn percentile_duration(
        samples: &[RenderSyncStats],
        index: usize,
        duration: impl Fn(RenderSyncStats) -> Duration,
    ) -> Duration {
        let mut durations = samples
            .iter()
            .map(|sample| duration(*sample))
            .collect::<Vec<_>>();
        durations.sort_unstable();
        durations[index]
    }

    fn max_duration(
        samples: &[RenderSyncStats],
        duration: impl Fn(RenderSyncStats) -> Duration,
    ) -> Duration {
        samples
            .iter()
            .map(|sample| duration(*sample))
            .max()
            .expect("render sync samples should not be empty")
    }

    fn print_render_sync_budget_stats(app: &mut App, stats: RenderSyncBudgetStats) {
        println!(
            "render_sync_small_visual_load_budget:\n  total: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  world_tiles: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  resources: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  placed_entities: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  belt_directions: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  belt_items: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  player: avg {:.3} ms, p95 {:.3} ms, max {:.3} ms\n  visible chunks: {}\n  visible placed entity sprites: {}\n  belt direction sprites: {}\n  belt item sprites: {}\n  resource sprites: {}\n  resource labels: {}",
            ms(stats.average.total),
            ms(stats.p95.total),
            ms(stats.max.total),
            ms(stats.average.world_tiles),
            ms(stats.p95.world_tiles),
            ms(stats.max.world_tiles),
            ms(stats.average.resources),
            ms(stats.p95.resources),
            ms(stats.max.resources),
            ms(stats.average.placed_entities),
            ms(stats.p95.placed_entities),
            ms(stats.max.placed_entities),
            ms(stats.average.belt_directions),
            ms(stats.p95.belt_directions),
            ms(stats.max.belt_directions),
            ms(stats.average.belt_items),
            ms(stats.p95.belt_items),
            ms(stats.max.belt_items),
            ms(stats.average.player),
            ms(stats.p95.player),
            ms(stats.max.player),
            app.world().resource::<VisibleChunks>().chunks.len(),
            placed_entity_sprite_count(app),
            belt_direction_sprite_count(app),
            belt_item_sprite_count(app),
            resource_sprite_count(app),
            resource_label_count(app),
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

    fn place_entities(
        sim: &mut Simulation,
        prototype_name: &str,
        count: usize,
        direction: Direction,
    ) -> Vec<factory_sim::EntityId> {
        let prototype_id = entity_prototype_id_by_name(sim.catalog(), prototype_name);
        let mut placed = Vec::with_capacity(count);

        for (x, y) in deterministic_tile_coords(sim) {
            if placed.len() == count {
                return placed;
            }
            if sim.can_place_entity(prototype_id, x, y, direction).is_err() {
                continue;
            }
            placed.push(
                sim.place_entity(prototype_id, x, y, direction)
                    .expect("validated render benchmark placement should succeed"),
            );
        }

        panic!(
            "could only place {} of {count} {prototype_name}",
            placed.len()
        );
    }

    fn deterministic_tile_coords(sim: &Simulation) -> Vec<(i32, i32)> {
        let mut chunks = sim.world().chunks.keys().copied().collect::<Vec<_>>();
        chunks.sort_unstable();
        chunks
            .into_iter()
            .flat_map(|coord| {
                (0..CHUNK_SIZE * CHUNK_SIZE).map(move |index| {
                    let local_x = index.rem_euclid(CHUNK_SIZE);
                    let local_y = index.div_euclid(CHUNK_SIZE);
                    (
                        coord.x * CHUNK_SIZE + local_x,
                        coord.y * CHUNK_SIZE + local_y,
                    )
                })
            })
            .collect()
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

    fn resource_label_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<ResourceAmountLabel>>()
            .iter(app.world())
            .count()
    }

    fn ms(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }
}
