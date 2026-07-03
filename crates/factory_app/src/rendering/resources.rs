use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_sim::{CHUNK_SIZE, ResourceCell, ResourceTileChange, Simulation};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::constants::{RESOURCE_SIZE, TILE_SIZE};
use crate::rendering::colors::{RenderPrototypeIds, resource_color};
use crate::rendering::transforms::tile_translation;
use crate::rendering::visuals::spawn_resource_visual;
use crate::resources::{RenderDetail, RenderSyncStats, SimResource, VisibleChunks};

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
    pub sprite_entities: HashMap<(i32, i32), Entity>,
    pub label_entities: HashMap<(i32, i32), Entity>,
    pub show_amount_labels: bool,
    pub expand_resource_sprites: bool,
}

pub(crate) fn sync_resource_debug_rendering(
    mut commands: Commands,
    mut params: ResourceRenderParams,
) {
    let resource_revision = params.sim.sim.world().resource_revision();
    let initial_sync = params.cache.last_resource_revision.is_none();
    let resources_changed = params.cache.last_resource_revision != Some(resource_revision);
    let visibility_changed = params.cache.last_visible_revision != params.visible.revision;
    let show_amount_labels =
        params.settings.show_amount_labels && params.detail.show_resource_amount_labels;
    let label_setting_changed = params.cache.show_amount_labels != show_amount_labels;
    let expand_resource_sprites = params.detail.expand_resource_sprites;
    let sprite_scale_changed = params.cache.expand_resource_sprites != expand_resource_sprites;

    if !initial_sync
        && !resources_changed
        && !visibility_changed
        && !label_setting_changed
        && !sprite_scale_changed
    {
        return;
    }

    let ids = RenderPrototypeIds::from_catalog(params.sim.sim.catalog());
    if initial_sync || visibility_changed || label_setting_changed || sprite_scale_changed {
        let resources = collect_resource_tiles(&params.sim.sim, &params.visible);
        reconcile_resource_tiles(
            &mut commands,
            &mut params.cache,
            &mut params.sprites,
            &mut params.labels,
            &resources,
            ResourceRenderContext {
                ids,
                show_amount_labels,
                expand_resource_sprites,
            },
        );
        params.cache.last_resource_revision = Some(resource_revision);
        params.cache.last_visible_revision = params.visible.revision;
        params.cache.show_amount_labels = show_amount_labels;
        params.cache.expand_resource_sprites = expand_resource_sprites;
        return;
    }

    if resources_changed {
        let last_revision = params
            .cache
            .last_resource_revision
            .expect("resource cache should be initialized before incremental sync");
        if let Some(changes) = params
            .sim
            .sim
            .world()
            .resource_dirty_tiles_since(last_revision)
        {
            for change in changes {
                apply_resource_tile_change(
                    &mut commands,
                    &mut params.cache,
                    &mut params.sprites,
                    &mut params.labels,
                    change,
                    ResourceTileChangeContext {
                        visible: &params.visible,
                        ids,
                        show_amount_labels,
                        expand_resource_sprites,
                    },
                );
            }
        } else {
            let resources = collect_resource_tiles(&params.sim.sim, &params.visible);
            reconcile_resource_tiles(
                &mut commands,
                &mut params.cache,
                &mut params.sprites,
                &mut params.labels,
                &resources,
                ResourceRenderContext {
                    ids,
                    show_amount_labels,
                    expand_resource_sprites,
                },
            );
        }
        params.cache.last_resource_revision = Some(resource_revision);
        params.cache.expand_resource_sprites = expand_resource_sprites;
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
    sprites:
        Query<'w, 's, (Entity, &'static mut Sprite, &'static mut Transform), With<ResourceSprite>>,
    labels: Query<'w, 's, (Entity, &'static mut Text2d), With<ResourceAmountLabel>>,
}

fn reconcile_resource_tiles(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    sprites: &mut Query<(Entity, &mut Sprite, &mut Transform), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    resources: &BTreeMap<(i32, i32), ResourceCell>,
    context: ResourceRenderContext,
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
        sync_resource_sprite(
            commands,
            cache,
            sprites,
            ResourceSpriteSync {
                x,
                y,
                resource,
                ids: context.ids,
                expand_resource_sprites: context.expand_resource_sprites,
            },
        );
    }

    if !context.show_amount_labels {
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
    sprites: &mut Query<(Entity, &mut Sprite, &mut Transform), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    change: ResourceTileChange,
    context: ResourceTileChangeContext,
) {
    let coord = (change.x, change.y);
    let chunk_coord = factory_sim::ChunkCoord {
        x: change.x.div_euclid(CHUNK_SIZE),
        y: change.y.div_euclid(CHUNK_SIZE),
    };
    if !context.visible.chunks.contains(&chunk_coord) {
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
        sprites,
        ResourceSpriteSync {
            x: change.x,
            y: change.y,
            resource,
            ids: context.ids,
            expand_resource_sprites: context.expand_resource_sprites,
        },
    );

    if context.show_amount_labels {
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
    expand_resource_sprites: bool,
}

#[derive(Clone, Copy)]
struct ResourceRenderContext {
    ids: RenderPrototypeIds,
    show_amount_labels: bool,
    expand_resource_sprites: bool,
}

#[derive(Clone, Copy)]
struct ResourceSpriteSync {
    x: i32,
    y: i32,
    resource: ResourceCell,
    ids: RenderPrototypeIds,
    expand_resource_sprites: bool,
}

fn sync_resource_sprite(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    sprites: &mut Query<(Entity, &mut Sprite, &mut Transform), With<ResourceSprite>>,
    sync: ResourceSpriteSync,
) {
    let coord = (sync.x, sync.y);
    if let Some(&entity) = cache.sprite_entities.get(&coord)
        && let Ok((_, mut sprite, mut transform)) = sprites.get_mut(entity)
    {
        sprite.color = resource_color(sync.resource, sync.ids);
        transform.scale = resource_sprite_scale(sync.expand_resource_sprites);
        return;
    }

    let entity = spawn_resource_sprite(commands, sync);
    cache.sprite_entities.insert(coord, entity);
}

fn sync_resource_label(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    x: i32,
    y: i32,
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

fn spawn_resource_sprite(commands: &mut Commands, sync: ResourceSpriteSync) -> Entity {
    let mut transform = Transform::from_translation(tile_translation(sync.x, sync.y, 1.0));
    transform.scale = resource_sprite_scale(sync.expand_resource_sprites);

    spawn_resource_visual(
        commands,
        resource_color(sync.resource, sync.ids),
        Vec2::splat(RESOURCE_SIZE),
        transform,
        ResourceSprite,
    )
}

fn resource_sprite_scale(expand_resource_sprites: bool) -> Vec3 {
    if expand_resource_sprites {
        Vec3::splat(TILE_SIZE / RESOURCE_SIZE * 1.08)
    } else {
        Vec3::ONE
    }
}

fn spawn_resource_label(commands: &mut Commands, x: i32, y: i32, resource: ResourceCell) -> Entity {
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
) -> BTreeMap<(i32, i32), ResourceCell> {
    let mut resources = BTreeMap::new();

    for coord in &visible.chunks {
        let Some(chunk) = sim.world().chunks.get(coord) else {
            continue;
        };
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if let Some(resource) = tile.resource {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                resources.insert(
                    (
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                    ),
                    resource,
                );
            }
        }
    }

    resources
}

pub(crate) fn format_resource_amount(amount: u32) -> String {
    amount.to_string()
}
