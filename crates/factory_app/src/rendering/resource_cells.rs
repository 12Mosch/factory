use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_sim::{CHUNK_SIZE, ChunkCoord, ResourceCell, Simulation};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

use crate::constants::RESOURCE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{RenderPrototypeIds, resource_color_variant};
use crate::rendering::resources::{RenderDetail, ResourcesRenderSyncTime};
use crate::rendering::transforms::tile_translation;
use crate::rendering::visuals::{VisualAssets, spawn_resource_visual};
use crate::resources::SimResource;
use crate::ui::accessibility::ReadableWorldLabel;

#[derive(Component)]
pub(crate) struct ResourceSprite;

#[derive(Component)]
pub(crate) struct ResourceAmountLabel;

/// One tile can create both a sprite and a label. Bounding tiles, instead of
/// individual ECS commands, keeps the result deterministic while preventing a
/// zoom or reload from materializing the whole resource field in one frame.
pub(crate) const RESOURCE_TILE_SYNC_BUDGET: usize = 512;
pub(crate) const RESOURCE_CHUNK_SCAN_BUDGET: usize = 8;

#[derive(Resource, Default)]
pub(crate) struct ResourceRenderSettings {
    pub(crate) show_amount_labels: bool,
}

#[derive(Resource, Default)]
pub struct ResourceRenderCache {
    pub last_resource_revision: Option<u64>,
    pub last_visible_revision: u64,
    pub sprite_entities:
        HashMap<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord), Entity>,
    pub label_entities: HashMap<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord), Entity>,
    pub show_amount_labels: bool,
    pub(crate) visible_chunks: BTreeSet<ChunkCoord>,
    pub(crate) pending_tiles: BTreeSet<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord)>,
    pub(crate) pending_chunk_scans: BTreeSet<ChunkCoord>,
    pub(crate) rendered_tiles_by_chunk:
        BTreeMap<ChunkCoord, BTreeSet<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord)>>,
    #[cfg(test)]
    pub(crate) tiles_processed_last_sync: usize,
}

pub(crate) fn sync_resource_debug_rendering(
    mut commands: Commands,
    mut params: ResourceRenderParams,
) {
    let sim = params.sim.read();
    let resource_revision = sim.world().resource_revision();
    let initial_sync = params.cache.last_resource_revision.is_none();
    let resources_changed = params.cache.last_resource_revision != Some(resource_revision);
    let visibility_changed = params.cache.last_visible_revision != params.visible.revision;
    let show_amount_labels =
        params.settings.show_amount_labels && params.detail.show_resource_amount_labels;
    let label_setting_changed = params.cache.show_amount_labels != show_amount_labels;

    if !initial_sync
        && !resources_changed
        && !visibility_changed
        && !label_setting_changed
        && params.cache.pending_tiles.is_empty()
        && params.cache.pending_chunk_scans.is_empty()
    {
        return;
    }

    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    if initial_sync || visibility_changed || label_setting_changed {
        if label_setting_changed {
            let rendered = params
                .cache
                .rendered_tiles_by_chunk
                .values()
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            params.cache.pending_tiles.extend(rendered);
        }

        let removed_chunks = params
            .cache
            .visible_chunks
            .difference(&params.visible.chunks)
            .copied()
            .collect::<Vec<_>>();
        for chunk in removed_chunks {
            if let Some(rendered) = params.cache.rendered_tiles_by_chunk.get(&chunk) {
                let rendered = rendered.iter().copied().collect::<Vec<_>>();
                params.cache.pending_tiles.extend(rendered);
            }
            params.cache.pending_chunk_scans.remove(&chunk);
        }

        let added_chunks = params
            .visible
            .chunks
            .difference(&params.cache.visible_chunks)
            .copied()
            .collect::<Vec<_>>();
        params.cache.pending_chunk_scans.extend(added_chunks);
        params.cache.visible_chunks = params.visible.chunks.clone();
        params.cache.last_visible_revision = params.visible.revision;
        params.cache.show_amount_labels = show_amount_labels;
    }

    if resources_changed && let Some(last_revision) = params.cache.last_resource_revision {
        if let Some(changes) = sim.world().resource_dirty_tiles_since(last_revision) {
            params
                .cache
                .pending_tiles
                .extend(changes.map(|change| (change.x, change.y)));
        } else {
            params
                .cache
                .pending_chunk_scans
                .extend(params.visible.chunks.iter().copied());
            let rendered = params
                .cache
                .rendered_tiles_by_chunk
                .values()
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            params.cache.pending_tiles.extend(rendered);
        }
    }

    params.cache.last_resource_revision = Some(resource_revision);
    let chunks_to_scan = params
        .cache
        .pending_chunk_scans
        .iter()
        .take(RESOURCE_CHUNK_SCAN_BUDGET)
        .copied()
        .collect::<Vec<_>>();
    for coord in chunks_to_scan {
        params.cache.pending_chunk_scans.remove(&coord);
        if !params.visible.chunks.contains(&coord) {
            continue;
        }
        let resources = collect_resource_tiles_in_chunk(&sim, coord);
        params.cache.pending_tiles.extend(resources.keys().copied());
    }

    let pending = params
        .cache
        .pending_tiles
        .iter()
        .take(RESOURCE_TILE_SYNC_BUDGET)
        .copied()
        .collect::<Vec<_>>();
    #[cfg(test)]
    {
        params.cache.tiles_processed_last_sync = pending.len();
    }
    for (x, y) in pending {
        params.cache.pending_tiles.remove(&(x, y));
        let resource = ChunkCoord::from_tile(x, y)
            .filter(|coord| params.visible.chunks.contains(coord))
            .and_then(|_| sim.world().tile_at(x, y))
            .and_then(|tile| tile.resource);
        apply_resource_tile_state(
            &mut commands,
            &mut params.cache,
            &mut params.visual_assets,
            &mut params.sprites,
            &mut params.labels,
            x,
            y,
            resource,
            ids,
            sim.seed(),
            show_amount_labels,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_resource_tile_state(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    visual_assets: &mut VisualAssets,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
    resource: Option<ResourceCell>,
    ids: RenderPrototypeIds,
    seed: u64,
    show_amount_labels: bool,
) {
    let coord = (x, y);
    let Some(resource) = resource else {
        if let Some(entity) = cache.sprite_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = cache.label_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        remove_rendered_tile(cache, coord);
        return;
    };

    sync_resource_sprite(
        commands,
        cache,
        visual_assets,
        sprites,
        x,
        y,
        resource,
        ids,
        seed,
    );
    if let Some(chunk) = ChunkCoord::from_tile(x, y) {
        cache
            .rendered_tiles_by_chunk
            .entry(chunk)
            .or_default()
            .insert(coord);
    }
    if show_amount_labels {
        sync_resource_label(commands, cache, labels, x, y, resource);
    } else if let Some(entity) = cache.label_entities.remove(&coord) {
        commands.entity(entity).despawn();
    }
}

fn remove_rendered_tile(
    cache: &mut ResourceRenderCache,
    coord: (factory_sim::WorldTileCoord, factory_sim::WorldTileCoord),
) {
    let Some(chunk) = ChunkCoord::from_tile(coord.0, coord.1) else {
        return;
    };
    let remove_chunk = cache
        .rendered_tiles_by_chunk
        .get_mut(&chunk)
        .is_some_and(|tiles| {
            tiles.remove(&coord);
            tiles.is_empty()
        });
    if remove_chunk {
        cache.rendered_tiles_by_chunk.remove(&chunk);
    }
}

pub(crate) fn measured_sync_resource_debug_rendering(
    commands: Commands,
    params: ResourceRenderParams,
    mut timing: ResMut<ResourcesRenderSyncTime>,
) {
    let started = Instant::now();
    sync_resource_debug_rendering(commands, params);
    timing.0 = started.elapsed();
}

#[derive(SystemParam)]
pub(crate) struct ResourceRenderParams<'w, 's> {
    sim: Res<'w, SimResource>,
    visible: Res<'w, VisibleChunks>,
    settings: Res<'w, ResourceRenderSettings>,
    detail: Res<'w, RenderDetail>,
    cache: ResMut<'w, ResourceRenderCache>,
    visual_assets: VisualAssets<'w>,
    sprites: Query<'w, 's, (Entity, &'static mut Sprite), With<ResourceSprite>>,
    labels: Query<'w, 's, (Entity, &'static mut Text2d), With<ResourceAmountLabel>>,
}

#[allow(clippy::too_many_arguments)]
fn sync_resource_sprite(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    visual_assets: &mut VisualAssets,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
    resource: ResourceCell,
    ids: RenderPrototypeIds,
    seed: u64,
) {
    let coord = (x, y);
    let color = resource_color_variant(resource, ids, seed, x, y);
    if let Some(&entity) = cache.sprite_entities.get(&coord)
        && let Ok((_, mut sprite)) = sprites.get_mut(entity)
    {
        *sprite = visual_assets.resource_sprite(color, Vec2::splat(RESOURCE_SIZE));
        return;
    }

    let entity = spawn_resource_sprite(commands, visual_assets, x, y, color);
    cache.sprite_entities.insert(coord, entity);
}

fn sync_resource_label(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
    resource: ResourceCell,
) {
    let coord = (x, y);
    if let Some(&entity) = cache.label_entities.get(&coord)
        && let Ok((_, mut text)) = labels.get_mut(entity)
    {
        text.0 = format_resource_amount(resource.amount);
        return;
    }

    let entity = spawn_resource_label(commands, x, y, resource);
    cache.label_entities.insert(coord, entity);
}

fn spawn_resource_sprite(
    commands: &mut Commands,
    visual_assets: &mut VisualAssets,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
    color: Color,
) -> Entity {
    spawn_resource_visual(
        commands,
        visual_assets,
        color,
        Vec2::splat(RESOURCE_SIZE),
        tile_translation(x, y, 1.0),
        ResourceSprite,
    )
}

fn spawn_resource_label(
    commands: &mut Commands,
    x: factory_sim::WorldTileCoord,
    y: factory_sim::WorldTileCoord,
    resource: ResourceCell,
) -> Entity {
    commands
        .spawn((
            Text2d::new(format_resource_amount(resource.amount)),
            TextFont::from_font_size(4.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            Transform::from_translation(tile_translation(x, y, 2.0)),
            Anchor::CENTER,
            Text2dShadow::default(),
            ReadableWorldLabel::new(4.0),
            ResourceAmountLabel,
        ))
        .id()
}

fn collect_resource_tiles_in_chunk(
    sim: &Simulation,
    coord: ChunkCoord,
) -> BTreeMap<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord), ResourceCell> {
    let mut resources = BTreeMap::new();
    let Some(chunk) = sim.world().chunks.get(&coord) else {
        return resources;
    };
    for (index, tile) in chunk.tiles.iter().enumerate() {
        if let Some(resource) = tile.resource {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            resources.insert(chunk.coord.tile_at(local_x, local_y), resource);
        }
    }
    resources
}

pub(crate) fn format_resource_amount(amount: u32) -> String {
    amount.to_string()
}
