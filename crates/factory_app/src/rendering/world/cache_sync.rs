use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use factory_sim::{CHUNK_SIZE, ChunkCoord};
use std::collections::BTreeSet;
use std::time::Instant;

use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{RenderPrototypeIds, TileColorTable};
use crate::rendering::resources::{RenderSyncStats, WorldRenderCache};
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;

use super::mesh::world_chunk_mesh;

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

pub(super) fn sync_visible_world_tiles_impl(
    commands: &mut Commands,
    params: WorldTilesRenderParams,
) {
    let WorldTilesRenderParams {
        sim,
        visible,
        token,
        mut cache,
        meshes,
        materials,
    } = params;
    let sim = sim.read();
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };

    if cache.last_reload_token == token.value
        && cache.last_visible_revision == visible.revision
        && cache.last_chunk_revision == sim.world().chunk_revision()
        && cache.last_terrain_revision == sim.world().terrain_revision()
    {
        return;
    }

    if cache.last_reload_token != token.value {
        for (_, entity) in std::mem::take(&mut cache.chunk_entities) {
            commands.entity(entity).despawn();
        }
        for (_, handle) in std::mem::take(&mut cache.chunk_meshes) {
            meshes.remove(handle.id());
        }
        cache.material = None;
        cache.last_reload_token = token.value;
    }

    let stale_chunks = cache
        .chunk_entities
        .keys()
        .copied()
        .filter(|coord| !visible.chunks.contains(coord) || !sim.world().chunks.contains_key(coord))
        .collect::<Vec<_>>();
    for coord in stale_chunks {
        if let Some(entity) = cache.chunk_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        if let Some(handle) = cache.chunk_meshes.remove(&coord) {
            meshes.remove(handle.id());
        }
    }

    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    let color_table = TileColorTable::from_catalog(sim.catalog());
    let material = cache
        .material
        .get_or_insert_with(|| {
            materials.add(ColorMaterial {
                alpha_mode: AlphaMode2d::Blend,
                ..Default::default()
            })
        })
        .clone();

    let mut stale_meshes = match sim
        .world()
        .chunk_generation_since(cache.last_chunk_revision)
    {
        Some(result) => cached_neighbors_of(result.generated_chunks(), &cache.chunk_meshes),
        None => cache.chunk_meshes.keys().copied().collect(),
    };
    match sim
        .world()
        .terrain_dirty_tiles_since(cache.last_terrain_revision)
    {
        Some(changes) => {
            for change in changes {
                add_chunks_affected_by_tile(
                    change.x,
                    change.y,
                    &cache.chunk_meshes,
                    &mut stale_meshes,
                );
            }
        }
        // The caller fell behind the bounded history; rebuild everything.
        None => stale_meshes.extend(cache.chunk_meshes.keys().copied()),
    }

    for coord in stale_meshes {
        let (Some(chunk), Some(handle)) = (
            sim.world().chunks.get(&coord),
            cache.chunk_meshes.get(&coord),
        ) else {
            continue;
        };
        meshes
            .insert(
                handle.id(),
                world_chunk_mesh(sim.world(), chunk, ids, &color_table),
            )
            .expect("cached chunk mesh handle should remain valid");
    }

    for coord in &visible.chunks {
        if cache.chunk_entities.contains_key(coord) {
            continue;
        }
        let Some(chunk) = sim.world().chunks.get(coord) else {
            continue;
        };
        let mesh = meshes.add(world_chunk_mesh(sim.world(), chunk, ids, &color_table));
        let entity = commands
            .spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::default(),
                WorldChunkMesh,
            ))
            .id();
        cache.chunk_entities.insert(*coord, entity);
        cache.chunk_meshes.insert(*coord, mesh);
    }

    cache.last_visible_revision = visible.revision;
    cache.last_chunk_revision = sim.world().chunk_revision();
    cache.last_terrain_revision = sim.world().terrain_revision();
}

/// Cached chunk meshes that a rewritten tile invalidates: its own chunk, plus
/// the cardinal neighbor across each chunk border the tile sits on. Water foam
/// is drawn from the neighboring tile, so filling a border tile changes the
/// adjacent chunk's mesh too.
fn add_chunks_affected_by_tile(
    x: i64,
    y: i64,
    cached_meshes: &std::collections::BTreeMap<ChunkCoord, Handle<Mesh>>,
    affected: &mut BTreeSet<ChunkCoord>,
) {
    let Some(coord) = ChunkCoord::from_tile(x, y) else {
        return;
    };
    if cached_meshes.contains_key(&coord) {
        affected.insert(coord);
    }

    let size = i64::from(CHUNK_SIZE);
    let local_x = x.rem_euclid(size);
    let local_y = y.rem_euclid(size);
    let border_offsets = [
        (local_x == 0).then_some((-1, 0)),
        (local_x == size - 1).then_some((1, 0)),
        (local_y == 0).then_some((0, -1)),
        (local_y == size - 1).then_some((0, 1)),
    ];
    for (dx, dy) in border_offsets.into_iter().flatten() {
        let (Some(nx), Some(ny)) = (coord.x.checked_add(dx), coord.y.checked_add(dy)) else {
            continue;
        };
        let neighbor = ChunkCoord { x: nx, y: ny };
        if cached_meshes.contains_key(&neighbor) {
            affected.insert(neighbor);
        }
    }
}

fn cached_neighbors_of(
    new_chunks: &[ChunkCoord],
    cached_meshes: &std::collections::BTreeMap<ChunkCoord, Handle<Mesh>>,
) -> BTreeSet<ChunkCoord> {
    const CARDINAL_OFFSETS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let mut affected = BTreeSet::new();
    for coord in new_chunks {
        for (dx, dy) in CARDINAL_OFFSETS {
            let Some(x) = coord.x.checked_add(dx) else {
                continue;
            };
            let Some(y) = coord.y.checked_add(dy) else {
                continue;
            };
            let neighbor = ChunkCoord { x, y };
            if cached_meshes.contains_key(&neighbor) {
                affected.insert(neighbor);
            }
        }
    }
    affected
}
