use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_sim::{CHUNK_SIZE, ResourceCell, ResourceTileChange, Simulation};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::constants::RESOURCE_SIZE;
use crate::map::resources::VisibleChunks;
use crate::rendering::colors::{RenderPrototypeIds, resource_color};
use crate::rendering::resources::{RenderDetail, RenderSyncStats};
use crate::rendering::transforms::tile_translation;
use crate::rendering::visuals::{VisualAssets, spawn_resource_visual};
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct ResourceSprite;

#[derive(Component)]
pub(crate) struct ResourceAmountLabel;

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

    if !initial_sync && !resources_changed && !visibility_changed && !label_setting_changed {
        return;
    }

    let ids = RenderPrototypeIds::from_catalog(sim.catalog());
    if initial_sync || visibility_changed || label_setting_changed {
        let resources = collect_resource_tiles(&sim, &params.visible);
        reconcile_resource_tiles(
            &mut commands,
            &mut params.cache,
            &mut params.visual_assets,
            &mut params.sprites,
            &mut params.labels,
            &resources,
            ids,
            show_amount_labels,
        );
        params.cache.last_resource_revision = Some(resource_revision);
        params.cache.last_visible_revision = params.visible.revision;
        params.cache.show_amount_labels = show_amount_labels;
        return;
    }

    if resources_changed {
        let last_revision = params
            .cache
            .last_resource_revision
            .expect("resource cache should be initialized before incremental sync");
        if let Some(changes) = sim.world().resource_dirty_tiles_since(last_revision) {
            for change in changes {
                apply_resource_tile_change(
                    &mut commands,
                    &mut params.cache,
                    &mut params.visual_assets,
                    &mut params.sprites,
                    &mut params.labels,
                    change,
                    ResourceTileChangeContext {
                        visible: &params.visible,
                        ids,
                        show_amount_labels,
                    },
                );
            }
        } else {
            let resources = collect_resource_tiles(&sim, &params.visible);
            reconcile_resource_tiles(
                &mut commands,
                &mut params.cache,
                &mut params.visual_assets,
                &mut params.sprites,
                &mut params.labels,
                &resources,
                ids,
                show_amount_labels,
            );
        }
        params.cache.last_resource_revision = Some(resource_revision);
    }
}

pub(crate) fn measured_sync_resource_debug_rendering(
    commands: Commands,
    params: ResourceRenderParams,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_resource_debug_rendering(commands, params);
    stats.record_resources(started.elapsed());
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
fn reconcile_resource_tiles(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    visual_assets: &mut VisualAssets,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    resources: &BTreeMap<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord), ResourceCell>,
    ids: RenderPrototypeIds,
    show_amount_labels: bool,
) {
    let stale_sprites = cache
        .sprite_entities
        .keys()
        .copied()
        .filter(|coord| !resources.contains_key(coord))
        .collect::<Vec<_>>();
    for coord in stale_sprites {
        if let Some(entity) = cache.sprite_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
    }

    for (&(x, y), &resource) in resources {
        sync_resource_sprite(commands, cache, visual_assets, sprites, x, y, resource, ids);
    }

    if !show_amount_labels {
        for (_, entity) in cache.label_entities.drain() {
            commands.entity(entity).despawn();
        }
        cache.show_amount_labels = false;
        return;
    }

    let stale_labels = cache
        .label_entities
        .keys()
        .copied()
        .filter(|coord| !resources.contains_key(coord))
        .collect::<Vec<_>>();
    for coord in stale_labels {
        if let Some(entity) = cache.label_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
    }

    for (&(x, y), &resource) in resources {
        sync_resource_label(commands, cache, labels, x, y, resource);
    }

    cache.show_amount_labels = true;
}

fn apply_resource_tile_change(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    visual_assets: &mut VisualAssets,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    change: ResourceTileChange,
    change_context: ResourceTileChangeContext,
) {
    let coord = (change.x, change.y);
    let Some(chunk_coord) = factory_sim::ChunkCoord::from_tile(change.x, change.y) else {
        return;
    };
    if !change_context.visible.chunks.contains(&chunk_coord) {
        if let Some(entity) = cache.sprite_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = cache.label_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(resource) = change.resource else {
        if let Some(entity) = cache.sprite_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = cache.label_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        return;
    };

    sync_resource_sprite(
        commands,
        cache,
        visual_assets,
        sprites,
        change.x,
        change.y,
        resource,
        change_context.ids,
    );

    if change_context.show_amount_labels {
        sync_resource_label(commands, cache, labels, change.x, change.y, resource);
    } else if let Some(entity) = cache.label_entities.remove(&coord) {
        commands.entity(entity).despawn();
    }
}

#[derive(Clone, Copy)]
struct ResourceTileChangeContext<'a> {
    visible: &'a VisibleChunks,
    ids: RenderPrototypeIds,
    show_amount_labels: bool,
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
) {
    let coord = (x, y);
    let color = resource_color(resource, ids);
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
            ResourceAmountLabel,
        ))
        .id()
}

pub(crate) fn collect_resource_tiles(
    sim: &Simulation,
    visible: &VisibleChunks,
) -> BTreeMap<(factory_sim::WorldTileCoord, factory_sim::WorldTileCoord), ResourceCell> {
    let mut resources = BTreeMap::new();

    for coord in &visible.chunks {
        let Some(chunk) = sim.world().chunks.get(coord) else {
            continue;
        };
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if let Some(resource) = tile.resource {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let tile_coord = chunk.coord.tile_at(local_x, local_y);
                resources.insert(tile_coord, resource);
            }
        }
    }

    resources
}

pub(crate) fn format_resource_amount(amount: u32) -> String {
    amount.to_string()
}
