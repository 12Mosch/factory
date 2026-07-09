use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
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
    }

    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    let material = cache
        .material
        .get_or_insert_with(|| materials.add(ColorMaterial::from_color(Color::WHITE)))
        .clone();

    for coord in &visible.chunks {
        if cache.chunk_entities.contains_key(coord) {
            continue;
        }
        let Some(chunk) = sim.world().chunks.get(coord) else {
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
    cache.last_chunk_revision = sim.world().chunk_revision();
}
