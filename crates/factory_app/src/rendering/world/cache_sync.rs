use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use factory_sim::{CHUNK_SIZE, ChunkCoord};
use std::collections::BTreeSet;
use std::time::Instant;

use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{RenderPrototypeIds, TileColorTable};
use crate::rendering::resources::{WorldRenderCache, WorldTilesRenderSyncTime};
use crate::resources::SimResource;
use crate::save_load::PresentationReloadToken;

use super::mesh::world_chunk_mesh;

#[derive(Component)]
pub struct WorldChunkMesh;

/// Maximum terrain meshes constructed or reconstructed by one render frame.
pub(crate) const WORLD_MESH_BUILD_BUDGET: usize = 16;
/// Hidden meshes retained after leaving the view, including adjacent prefetch.
pub(crate) const INACTIVE_MESH_CACHE_CAPACITY: usize = 128;
const PREFETCH_OFFSETS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

pub(crate) fn measured_sync_visible_world_tiles(
    commands: Commands,
    params: WorldTilesRenderParams,
    mut timing: ResMut<WorldTilesRenderSyncTime>,
) {
    let started = Instant::now();
    sync_visible_world_tiles(commands, params);
    timing.0 = started.elapsed();
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

    #[cfg(test)]
    {
        cache.mesh_builds_last_sync = 0;
        cache.mesh_cache_hits_last_sync = 0;
    }

    if cache.last_reload_token == token.value
        && cache.last_visible_revision == visible.revision
        && cache.last_chunk_revision == sim.world().chunk_revision()
        && cache.last_terrain_revision == sim.world().terrain_revision()
        && cache.pending_mesh_builds.is_empty()
        && cache.pending_mesh_rebuilds.is_empty()
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
        cache.pending_mesh_builds.clear();
        cache.pending_mesh_rebuilds.clear();
        cache.inactive_mesh_lru.clear();
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
        if sim.world().chunks.contains_key(&coord) {
            touch_inactive_mesh(&mut cache, coord);
        } else {
            remove_cached_mesh(&mut cache, &mut meshes, coord);
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

    if cache.last_chunk_revision != sim.world().chunk_revision() {
        match sim
            .world()
            .chunk_generation_since(cache.last_chunk_revision)
        {
            Some(result) => {
                let stale = cached_neighbors_of(result.generated_chunks(), &cache.chunk_meshes);
                cache.pending_mesh_rebuilds.extend(stale);
            }
            None => {
                let stale = cache.chunk_meshes.keys().copied().collect::<Vec<_>>();
                cache.pending_mesh_rebuilds.extend(stale);
            }
        }
    }
    if cache.last_terrain_revision != sim.world().terrain_revision() {
        match sim
            .world()
            .terrain_dirty_tiles_since(cache.last_terrain_revision)
        {
            Some(changes) => {
                for change in changes {
                    let mut stale = BTreeSet::new();
                    add_chunks_affected_by_tile(
                        change.x,
                        change.y,
                        &cache.chunk_meshes,
                        &mut stale,
                    );
                    cache.pending_mesh_rebuilds.extend(stale);
                }
            }
            // The caller fell behind the bounded history; rebuild cached
            // meshes gradually instead of creating a single catch-up spike.
            None => {
                let stale = cache.chunk_meshes.keys().copied().collect::<Vec<_>>();
                cache.pending_mesh_rebuilds.extend(stale);
            }
        }
    }

    if cache.last_visible_revision != visible.revision {
        cache.pending_mesh_builds.clear();
        let missing = visible
            .chunks
            .iter()
            .copied()
            .filter(|coord| !cache.chunk_meshes.contains_key(coord))
            .collect::<Vec<_>>();
        cache.pending_mesh_builds.extend(missing);
        for coord in adjacent_chunks(&visible.chunks) {
            if sim.world().chunks.contains_key(&coord) && !cache.chunk_meshes.contains_key(&coord) {
                cache.pending_mesh_builds.insert(coord);
            }
        }
    }

    // A newly generated adjacent chunk becomes prefetchable even when the
    // camera did not move and therefore did not advance the visible revision.
    if cache.last_chunk_revision != sim.world().chunk_revision() {
        for coord in adjacent_chunks(&visible.chunks) {
            if sim.world().chunks.contains_key(&coord) && !cache.chunk_meshes.contains_key(&coord) {
                cache.pending_mesh_builds.insert(coord);
            }
        }
    }

    let mut build_budget = WORLD_MESH_BUILD_BUDGET;
    let visible_missing = cache
        .pending_mesh_builds
        .iter()
        .filter(|coord| visible.chunks.contains(coord))
        .take(build_budget)
        .copied()
        .collect::<Vec<_>>();
    for coord in visible_missing {
        build_new_mesh(&sim, &mut cache, &mut meshes, coord, ids, &color_table);
        build_budget -= 1;
    }

    let visible_rebuilds = cache
        .pending_mesh_rebuilds
        .iter()
        .filter(|coord| visible.chunks.contains(coord))
        .take(build_budget)
        .copied()
        .collect::<Vec<_>>();
    for coord in visible_rebuilds {
        rebuild_cached_mesh(&sim, &mut cache, &mut meshes, coord, ids, &color_table);
        build_budget -= 1;
    }

    let background_rebuilds = cache
        .pending_mesh_rebuilds
        .iter()
        .take(build_budget)
        .copied()
        .collect::<Vec<_>>();
    for coord in background_rebuilds {
        rebuild_cached_mesh(&sim, &mut cache, &mut meshes, coord, ids, &color_table);
        build_budget -= 1;
    }

    let prefetch = cache
        .pending_mesh_builds
        .iter()
        .take(build_budget)
        .copied()
        .collect::<Vec<_>>();
    for coord in prefetch {
        build_new_mesh(&sim, &mut cache, &mut meshes, coord, ids, &color_table);
        touch_inactive_mesh(&mut cache, coord);
    }

    for coord in &visible.chunks {
        if cache.chunk_entities.contains_key(coord) {
            continue;
        }
        if !sim.world().chunks.contains_key(coord) {
            continue;
        }
        let Some(mesh) = cache.chunk_meshes.get(coord).cloned() else {
            // Still waiting behind the per-frame mesh construction budget.
            continue;
        };
        if let Some(position) = cache
            .inactive_mesh_lru
            .iter()
            .position(|cached| cached == coord)
        {
            cache.inactive_mesh_lru.remove(position);
            #[cfg(test)]
            {
                cache.mesh_cache_hits_last_sync += 1;
            }
        }
        let entity = commands
            .spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::default(),
                WorldChunkMesh,
            ))
            .id();
        cache.chunk_entities.insert(*coord, entity);
    }

    prune_inactive_meshes(&mut cache, &mut meshes);

    cache.last_visible_revision = visible.revision;
    cache.last_chunk_revision = sim.world().chunk_revision();
    cache.last_terrain_revision = sim.world().terrain_revision();
}

fn build_new_mesh(
    sim: &factory_sim::Simulation,
    cache: &mut WorldRenderCache,
    meshes: &mut Assets<Mesh>,
    coord: ChunkCoord,
    ids: RenderPrototypeIds,
    color_table: &TileColorTable,
) {
    cache.pending_mesh_builds.remove(&coord);
    let Some(chunk) = sim.world().chunks.get(&coord) else {
        return;
    };
    let mesh = meshes.add(world_chunk_mesh(sim.world(), chunk, ids, color_table));
    cache.chunk_meshes.insert(coord, mesh);
    cache.pending_mesh_rebuilds.remove(&coord);
    #[cfg(test)]
    {
        cache.mesh_builds_last_sync += 1;
    }
}

fn rebuild_cached_mesh(
    sim: &factory_sim::Simulation,
    cache: &mut WorldRenderCache,
    meshes: &mut Assets<Mesh>,
    coord: ChunkCoord,
    ids: RenderPrototypeIds,
    color_table: &TileColorTable,
) {
    cache.pending_mesh_rebuilds.remove(&coord);
    let (Some(chunk), Some(handle)) = (
        sim.world().chunks.get(&coord),
        cache.chunk_meshes.get(&coord),
    ) else {
        return;
    };
    meshes
        .insert(
            handle.id(),
            world_chunk_mesh(sim.world(), chunk, ids, color_table),
        )
        .expect("cached chunk mesh handle should remain valid");
    #[cfg(test)]
    {
        cache.mesh_builds_last_sync += 1;
    }
}

fn adjacent_chunks(visible: &BTreeSet<ChunkCoord>) -> BTreeSet<ChunkCoord> {
    let mut adjacent = BTreeSet::new();
    for coord in visible {
        for (dx, dy) in PREFETCH_OFFSETS {
            let (Some(x), Some(y)) = (coord.x.checked_add(dx), coord.y.checked_add(dy)) else {
                continue;
            };
            let candidate = ChunkCoord { x, y };
            if !visible.contains(&candidate) {
                adjacent.insert(candidate);
            }
        }
    }
    adjacent
}

fn touch_inactive_mesh(cache: &mut WorldRenderCache, coord: ChunkCoord) {
    if let Some(position) = cache
        .inactive_mesh_lru
        .iter()
        .position(|cached| *cached == coord)
    {
        cache.inactive_mesh_lru.remove(position);
    }
    cache.inactive_mesh_lru.push_back(coord);
}

fn remove_cached_mesh(cache: &mut WorldRenderCache, meshes: &mut Assets<Mesh>, coord: ChunkCoord) {
    if let Some(handle) = cache.chunk_meshes.remove(&coord) {
        meshes.remove(handle.id());
    }
    cache.pending_mesh_builds.remove(&coord);
    cache.pending_mesh_rebuilds.remove(&coord);
    cache.inactive_mesh_lru.retain(|cached| *cached != coord);
}

fn prune_inactive_meshes(cache: &mut WorldRenderCache, meshes: &mut Assets<Mesh>) {
    while cache.inactive_mesh_lru.len() > INACTIVE_MESH_CACHE_CAPACITY {
        let Some(coord) = cache.inactive_mesh_lru.pop_front() else {
            break;
        };
        if !cache.chunk_entities.contains_key(&coord) {
            remove_cached_mesh(cache, meshes, coord);
        }
    }
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
