use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use factory_sim::ChunkCoord;
use std::collections::BTreeSet;
use std::time::Instant;

use crate::map::resources::VisibleChunks;
use crate::rendering::colors::RenderPrototypeIds;
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
        cache.known_generated_chunks.clear();
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
    let material = cache
        .material
        .get_or_insert_with(|| {
            materials.add(ColorMaterial {
                alpha_mode: AlphaMode2d::Blend,
                ..Default::default()
            })
        })
        .clone();

    let new_chunks = sim
        .world()
        .chunks
        .keys()
        .copied()
        .filter(|coord| !cache.known_generated_chunks.contains(coord))
        .collect::<Vec<_>>();
    let affected_cached_neighbors = cached_neighbors_of(&new_chunks, &cache.chunk_meshes);
    for coord in affected_cached_neighbors {
        let (Some(chunk), Some(handle)) = (
            sim.world().chunks.get(&coord),
            cache.chunk_meshes.get(&coord),
        ) else {
            continue;
        };
        meshes
            .insert(handle.id(), world_chunk_mesh(sim.world(), chunk, ids))
            .expect("cached chunk mesh handle should remain valid");
    }

    for coord in &visible.chunks {
        if cache.chunk_entities.contains_key(coord) {
            continue;
        }
        let Some(chunk) = sim.world().chunks.get(coord) else {
            continue;
        };
        let mesh = meshes.add(world_chunk_mesh(sim.world(), chunk, ids));
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

    cache.known_generated_chunks = sim.world().chunks.keys().copied().collect();
    cache.last_visible_revision = visible.revision;
    cache.last_chunk_revision = sim.world().chunk_revision();
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
