use bevy::prelude::*;
use bevy::sprite::{Anchor, Text2dShadow};
use factory_sim::{CHUNK_SIZE, ResourceCell, ResourceTileChange, Simulation};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use crate::constants::RESOURCE_SIZE;
use crate::rendering::colors::{RenderPrototypeIds, resource_color};
use crate::rendering::transforms::tile_translation;
use crate::resources::{RenderSyncStats, SimResource};

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
    pub sprite_entities: HashMap<(i32, i32), Entity>,
    pub label_entities: HashMap<(i32, i32), Entity>,
    pub show_amount_labels: bool,
}

pub(crate) fn sync_resource_debug_rendering(
    mut commands: Commands,
    sim: Res<SimResource>,
    settings: Res<ResourceRenderSettings>,
    mut cache: ResMut<ResourceRenderCache>,
    mut sprites: Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    mut labels: Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
) {
    let resource_revision = sim.sim.world().resource_revision();
    let initial_sync = cache.last_resource_revision.is_none();
    let resources_changed = cache.last_resource_revision != Some(resource_revision);
    let label_setting_changed = cache.show_amount_labels != settings.show_amount_labels;

    if !initial_sync && !resources_changed && !label_setting_changed {
        return;
    }

    let ids = RenderPrototypeIds::from_catalog(sim.sim.catalog());
    if initial_sync {
        let resources = collect_resource_tiles(&sim.sim);
        full_sync_resources(
            &mut commands,
            &mut cache,
            &mut sprites,
            &mut labels,
            &resources,
            ids,
            settings.show_amount_labels,
        );
        cache.last_resource_revision = Some(resource_revision);
        cache.show_amount_labels = settings.show_amount_labels;
        return;
    }

    if label_setting_changed && !settings.show_amount_labels {
        for (entity, _) in &mut labels {
            commands.entity(entity).despawn();
        }
        cache.label_entities.clear();
        cache.show_amount_labels = false;
    }

    if resources_changed {
        let last_revision = cache
            .last_resource_revision
            .expect("resource cache should be initialized before incremental sync");
        if let Some(changes) = sim.sim.world().resource_dirty_tiles_since(last_revision) {
            for change in changes {
                apply_resource_tile_change(
                    &mut commands,
                    &mut cache,
                    &mut sprites,
                    &mut labels,
                    change,
                    ids,
                    settings.show_amount_labels,
                );
            }
        } else {
            let resources = collect_resource_tiles(&sim.sim);
            full_sync_resources(
                &mut commands,
                &mut cache,
                &mut sprites,
                &mut labels,
                &resources,
                ids,
                settings.show_amount_labels,
            );
        }
        cache.last_resource_revision = Some(resource_revision);
    }

    if label_setting_changed && settings.show_amount_labels {
        let resources = collect_resource_tiles(&sim.sim);
        full_sync_resource_labels(&mut commands, &mut cache, &mut labels, &resources);
        cache.show_amount_labels = true;
    }
}

pub(crate) fn measured_sync_resource_debug_rendering(
    commands: Commands,
    sim: Res<SimResource>,
    settings: Res<ResourceRenderSettings>,
    cache: ResMut<ResourceRenderCache>,
    sprites: Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    labels: Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_resource_debug_rendering(commands, sim, settings, cache, sprites, labels);
    stats.record_resources(started.elapsed());
}

fn full_sync_resources(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    resources: &BTreeMap<(i32, i32), ResourceCell>,
    ids: RenderPrototypeIds,
    show_amount_labels: bool,
) {
    for (entity, _) in sprites.iter_mut() {
        commands.entity(entity).despawn();
    }
    for (entity, _) in labels.iter_mut() {
        commands.entity(entity).despawn();
    }

    cache.sprite_entities.clear();
    cache.label_entities.clear();
    cache.show_amount_labels = show_amount_labels;

    for (&(x, y), &resource) in resources {
        let entity = spawn_resource_sprite(commands, x, y, resource, ids);
        cache.sprite_entities.insert((x, y), entity);

        if show_amount_labels {
            let entity = spawn_resource_label(commands, x, y, resource);
            cache.label_entities.insert((x, y), entity);
        }
    }
}

fn full_sync_resource_labels(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    resources: &BTreeMap<(i32, i32), ResourceCell>,
) {
    for (entity, _) in labels.iter_mut() {
        commands.entity(entity).despawn();
    }

    cache.label_entities.clear();
    for (&(x, y), &resource) in resources {
        let entity = spawn_resource_label(commands, x, y, resource);
        cache.label_entities.insert((x, y), entity);
    }
}

fn apply_resource_tile_change(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    labels: &mut Query<(Entity, &mut Text2d), With<ResourceAmountLabel>>,
    change: ResourceTileChange,
    ids: RenderPrototypeIds,
    show_amount_labels: bool,
) {
    let coord = (change.x, change.y);
    let Some(resource) = change.resource else {
        if let Some(entity) = cache.sprite_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        if let Some(entity) = cache.label_entities.remove(&coord) {
            commands.entity(entity).despawn();
        }
        return;
    };

    sync_resource_sprite(commands, cache, sprites, change.x, change.y, resource, ids);

    if show_amount_labels {
        sync_resource_label(commands, cache, labels, change.x, change.y, resource);
    } else if let Some(entity) = cache.label_entities.remove(&coord) {
        commands.entity(entity).despawn();
    }
}

fn sync_resource_sprite(
    commands: &mut Commands,
    cache: &mut ResourceRenderCache,
    sprites: &mut Query<(Entity, &mut Sprite), With<ResourceSprite>>,
    x: i32,
    y: i32,
    resource: ResourceCell,
    ids: RenderPrototypeIds,
) {
    let coord = (x, y);
    if let Some(&entity) = cache.sprite_entities.get(&coord)
        && let Ok((_, mut sprite)) = sprites.get_mut(entity)
    {
        sprite.color = resource_color(resource, ids);
        return;
    }

    let entity = spawn_resource_sprite(commands, x, y, resource, ids);
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

fn spawn_resource_sprite(
    commands: &mut Commands,
    x: i32,
    y: i32,
    resource: ResourceCell,
    ids: RenderPrototypeIds,
) -> Entity {
    commands
        .spawn((
            Sprite::from_color(resource_color(resource, ids), Vec2::splat(RESOURCE_SIZE)),
            Transform::from_translation(tile_translation(x, y, 1.0)),
            ResourceSprite,
        ))
        .id()
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

pub(crate) fn collect_resource_tiles(sim: &Simulation) -> BTreeMap<(i32, i32), ResourceCell> {
    let mut resources = BTreeMap::new();

    for chunk in sim.world().chunks.values() {
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
