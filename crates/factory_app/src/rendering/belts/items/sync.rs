use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::BasePrototypeIds;
use factory_sim::{BeltItemId, EntityId, Simulation};
use std::collections::HashSet;
use std::time::Instant;

use crate::constants::BELT_ITEM_SPRITE_SIZE;
use crate::rendering::resources::{
    BeltItemRenderPool, RenderDetail, RenderSyncStats, VisibleEntityIds,
};
use crate::rendering::visuals::{VisualAssets, spawn_belt_item_visual};
use crate::resources::SimResource;

use super::super::components::{BeltItemLabel, BeltItemSprite, VisibleBeltItemRenderState};
use super::super::labels::{belt_item_label, label_translation, spawn_or_reuse_belt_item_label};
use super::cache::{BeltItemRenderCache, CachedBelt, CachedBeltItem};
use super::collection::collect_belt_items_into;

#[derive(Default)]
struct BeltItemRenderScratch {
    visible_items: Vec<VisibleBeltItemRenderState>,
    changed_belts: Vec<EntityId>,
    removed_items: Vec<(EntityId, BeltItemId)>,
    interpolated_items: usize,
}

#[derive(SystemParam)]
pub(crate) struct BeltItemRenderParams<'w, 's> {
    commands: Commands<'w, 's>,
    sim: Res<'w, SimResource>,
    visible_entity_ids: Res<'w, VisibleEntityIds>,
    detail: Res<'w, RenderDetail>,
    pool: ResMut<'w, BeltItemRenderPool>,
    visual_assets: VisualAssets<'w>,
    scratch: Local<'s, BeltItemRenderScratch>,
    cache: Local<'s, BeltItemRenderCache>,
    fixed_time: Option<Res<'w, Time<Fixed>>>,
    sprites: Query<
        'w,
        's,
        (
            Entity,
            &'static mut BeltItemSprite,
            &'static mut Transform,
            &'static mut Sprite,
            &'static mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: Query<
        'w,
        's,
        (
            Entity,
            &'static mut BeltItemLabel,
            &'static mut Transform,
            &'static mut Text2d,
            &'static mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
}

pub(crate) fn sync_belt_item_rendering(params: BeltItemRenderParams) {
    let BeltItemRenderParams {
        mut commands,
        sim,
        visible_entity_ids,
        detail,
        mut pool,
        mut visual_assets,
        mut scratch,
        mut cache,
        fixed_time,
        mut sprites,
        mut labels,
    } = params;

    if !detail.show_belt_items {
        if detail.is_changed() && cache.has_items() {
            pool_cached_belt_items(&mut cache, &mut pool, &mut sprites, &mut labels);
        }
        return;
    }

    let sim_replaced = cache.sim_replacement_revision() != sim.replacement_revision();
    if sim_replaced {
        cache.set_sim_replacement_revision(sim.replacement_revision());
        pool_cached_belt_items(&mut cache, &mut pool, &mut sprites, &mut labels);
    }
    let alpha = fixed_time
        .as_deref()
        .map_or(1.0, Time::<Fixed>::overstep_fraction)
        .clamp(0.0, 1.0);
    let interpolation_frame = cache.advance_interpolation_frame();
    scratch.interpolated_items = 0;
    let sim = sim.read();
    let ids = BasePrototypeIds::from_catalog(sim.catalog());
    let visibility_changed = visible_entity_ids.is_changed() || detail.is_changed() || sim_replaced;
    let items_changed = cache.last_item_revision() != sim.belt_item_revision();
    if visibility_changed || items_changed {
        collect_changed_belts(
            &sim,
            &visible_entity_ids.ids,
            visibility_changed,
            sim_replaced,
            &cache,
            &mut scratch.changed_belts,
        );
        sync_changed_belts(
            &mut commands,
            &sim,
            ids,
            &visible_entity_ids.ids,
            detail.show_belt_item_labels,
            alpha,
            interpolation_frame,
            &mut pool,
            &mut visual_assets,
            &mut scratch,
            &mut cache,
            &mut sprites,
            &mut labels,
        );
        cache.set_last_item_revision(sim.belt_item_revision());
    }

    interpolate_belt_items(
        alpha,
        interpolation_frame,
        scratch.interpolated_items,
        &mut cache,
        &mut sprites,
        &mut labels,
    );
}

fn collect_changed_belts(
    sim: &Simulation,
    visible_ids: &HashSet<EntityId>,
    visibility_changed: bool,
    force_all: bool,
    cache: &BeltItemRenderCache,
    changed_belts: &mut Vec<EntityId>,
) {
    changed_belts.clear();
    for &entity_id in visible_ids {
        let revision = sim.belt_entity_item_revision(entity_id);
        if force_all
            || (visibility_changed && cache.belt(entity_id).is_none())
            || cache
                .belt(entity_id)
                .is_some_and(|belt| belt.revision != revision)
        {
            changed_belts.push(entity_id);
        }
    }
    if visibility_changed {
        changed_belts.extend(
            cache.belts().filter_map(|(entity_id, _)| {
                (!visible_ids.contains(&entity_id)).then_some(entity_id)
            }),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_changed_belts(
    commands: &mut Commands,
    sim: &Simulation,
    ids: BasePrototypeIds,
    visible_ids: &HashSet<EntityId>,
    show_labels: bool,
    alpha: f32,
    interpolation_frame: u64,
    pool: &mut BeltItemRenderPool,
    visual_assets: &mut VisualAssets,
    scratch: &mut BeltItemRenderScratch,
    cache: &mut BeltItemRenderCache,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    scratch.removed_items.clear();
    for changed_index in 0..scratch.changed_belts.len() {
        let entity_id = scratch.changed_belts[changed_index];
        let Some(mut old_belt) = cache.take_belt(entity_id) else {
            if visible_ids.contains(&entity_id) {
                sync_current_belt(
                    commands,
                    sim,
                    ids,
                    entity_id,
                    show_labels,
                    alpha,
                    interpolation_frame,
                    pool,
                    visual_assets,
                    scratch,
                    cache,
                    sprites,
                    labels,
                    CachedBelt::default(),
                );
            }
            continue;
        };
        if !visible_ids.contains(&entity_id) {
            scratch.removed_items.extend(
                old_belt
                    .item_ids
                    .drain(..)
                    .map(|item_id| (entity_id, item_id)),
            );
            continue;
        }
        sync_current_belt(
            commands,
            sim,
            ids,
            entity_id,
            show_labels,
            alpha,
            interpolation_frame,
            pool,
            visual_assets,
            scratch,
            cache,
            sprites,
            labels,
            old_belt,
        );
    }

    for &(owner, item_id) in &scratch.removed_items {
        if cache.item(item_id).is_some_and(|item| item.owner == owner)
            && !cache
                .belt(owner)
                .is_some_and(|belt| belt.item_ids.contains(&item_id))
            && let Some(item) = cache.remove_item(item_id)
        {
            pool_cached_item(item, pool, sprites, labels);
        }
    }

    if cache.labels_visible() != show_labels {
        sync_label_visibility(commands, sim, show_labels, pool, cache, labels);
        cache.set_labels_visible(show_labels);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_current_belt(
    commands: &mut Commands,
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    show_labels: bool,
    alpha: f32,
    interpolation_frame: u64,
    pool: &mut BeltItemRenderPool,
    visual_assets: &mut VisualAssets,
    scratch: &mut BeltItemRenderScratch,
    cache: &mut BeltItemRenderCache,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
    mut old_belt: CachedBelt,
) {
    collect_belt_items_into(sim, ids, entity_id, &mut scratch.visible_items);
    scratch.removed_items.extend(
        old_belt
            .item_ids
            .iter()
            .copied()
            .map(|item_id| (entity_id, item_id)),
    );
    old_belt.revision = sim.belt_entity_item_revision(entity_id);
    old_belt.item_ids.clear();
    for item in scratch.visible_items.iter().copied() {
        old_belt.item_ids.push(item.key);
        let reused_cached_item = if let Some(cached) = cache.item_mut(item.key) {
            cached.owner = entity_id;
            cached.previous_translation = cached.target_translation;
            cached.target_translation = item.translation;
            let item_type_changed = cached.item_id != item.item_id;
            if item_type_changed {
                cached.item_id = item.item_id;
            }
            let translation = cached
                .previous_translation
                .lerp(cached.target_translation, alpha);
            match sprites.get_mut(cached.sprite) {
                Ok((_, mut marker, mut transform, mut sprite, _)) => {
                    transform.translation = translation;
                    if item_type_changed {
                        marker.item_id = item.item_id;
                        *sprite = visual_assets
                            .belt_item_sprite(item.color, Vec2::splat(BELT_ITEM_SPRITE_SIZE));
                    }
                    if let Some(label_entity) = cached.label
                        && let Ok((_, mut marker, mut transform, mut text, _)) =
                            labels.get_mut(label_entity)
                    {
                        transform.translation = label_translation(translation);
                        if item_type_changed {
                            marker.item_id = item.item_id;
                            text.0 = belt_item_label(sim, item.item_id);
                        }
                    }
                    cached.interpolation_frame = interpolation_frame;
                    scratch.interpolated_items += 1;
                    true
                }
                Err(_) => false,
            }
        } else {
            false
        };
        if reused_cached_item {
            continue;
        }
        if let Some(stale_item) = cache.remove_item(item.key) {
            pool_cached_item(stale_item, pool, sprites, labels);
        }

        let sprite = spawn_or_reuse_belt_item_sprite(commands, pool, visual_assets, item);
        let label = show_labels.then(|| spawn_or_reuse_belt_item_label(commands, sim, pool, item));
        cache.insert_item(
            item.key,
            CachedBeltItem {
                owner: entity_id,
                item_id: item.item_id,
                sprite,
                label,
                previous_translation: item.translation,
                target_translation: item.translation,
                interpolation_frame,
            },
        );
        scratch.interpolated_items += 1;
    }
    cache.insert_belt(entity_id, old_belt);
}

fn interpolate_belt_items(
    alpha: f32,
    interpolation_frame: u64,
    already_interpolated: usize,
    cache: &mut BeltItemRenderCache,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    if already_interpolated == cache.item_count() {
        return;
    }
    for (key, item) in cache.items_mut() {
        if item.interpolation_frame == interpolation_frame {
            continue;
        }
        let translation = item
            .previous_translation
            .lerp(item.target_translation, alpha);
        if let Ok((_, marker, mut transform, _, _)) = sprites.get_mut(item.sprite) {
            debug_assert_eq!(marker.key, key);
            transform.translation = translation;
        }
        if let Some(label) = item.label
            && let Ok((_, marker, mut transform, _, _)) = labels.get_mut(label)
        {
            debug_assert_eq!(marker.key, key);
            transform.translation = label_translation(translation);
        }
    }
}

fn sync_label_visibility(
    commands: &mut Commands,
    sim: &Simulation,
    show_labels: bool,
    pool: &mut BeltItemRenderPool,
    cache: &mut BeltItemRenderCache,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    for (key, item) in cache.items_mut() {
        if show_labels && item.label.is_none() {
            let render_state = VisibleBeltItemRenderState {
                key,
                item_id: item.item_id,
                translation: item.target_translation,
                color: Color::NONE,
            };
            item.label = Some(spawn_or_reuse_belt_item_label(
                commands,
                sim,
                pool,
                render_state,
            ));
        } else if !show_labels
            && let Some(entity) = item.label.take()
            && let Ok((_, mut marker, _, _, mut visibility)) = labels.get_mut(entity)
            && deactivate(&mut marker.active)
        {
            *visibility = Visibility::Hidden;
            pool.labels.push(entity);
        }
    }
}

fn pool_cached_belt_items(
    cache: &mut BeltItemRenderCache,
    pool: &mut BeltItemRenderPool,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    for item in cache.take_items() {
        pool_cached_item(item, pool, sprites, labels);
    }
    cache.clear_belts();
}

fn pool_cached_item(
    item: CachedBeltItem,
    pool: &mut BeltItemRenderPool,
    sprites: &mut Query<
        (
            Entity,
            &mut BeltItemSprite,
            &mut Transform,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<BeltItemLabel>,
    >,
    labels: &mut Query<
        (
            Entity,
            &mut BeltItemLabel,
            &mut Transform,
            &mut Text2d,
            &mut Visibility,
        ),
        Without<BeltItemSprite>,
    >,
) {
    if let Ok((_, mut marker, _, _, mut visibility)) = sprites.get_mut(item.sprite)
        && deactivate(&mut marker.active)
    {
        *visibility = Visibility::Hidden;
        pool.sprites.push(item.sprite);
    }
    if let Some(label) = item.label
        && let Ok((_, mut marker, _, _, mut visibility)) = labels.get_mut(label)
        && deactivate(&mut marker.active)
    {
        *visibility = Visibility::Hidden;
        pool.labels.push(label);
    }
}

pub(crate) fn measured_sync_belt_item_rendering(
    params: BeltItemRenderParams,
    mut stats: ResMut<RenderSyncStats>,
) {
    let started = Instant::now();
    sync_belt_item_rendering(params);
    stats.record_belt_items(started.elapsed());
}

pub(super) fn spawn_or_reuse_belt_item_sprite(
    commands: &mut Commands,
    pool: &mut BeltItemRenderPool,
    visual_assets: &mut VisualAssets,
    item: VisibleBeltItemRenderState,
) -> Entity {
    let marker = BeltItemSprite {
        key: item.key,
        item_id: item.item_id,
        active: true,
    };

    if let Some(entity) = pool.sprites.pop() {
        commands.entity(entity).insert((
            visual_assets.belt_item_sprite(item.color, Vec2::splat(BELT_ITEM_SPRITE_SIZE)),
            Transform::from_translation(item.translation),
            Visibility::Visible,
            marker,
        ));
        return entity;
    }

    spawn_belt_item_visual(
        commands,
        visual_assets,
        item.color,
        Vec2::splat(BELT_ITEM_SPRITE_SIZE),
        item.translation,
        (marker, Visibility::Visible),
    )
}

fn deactivate(active: &mut bool) -> bool {
    std::mem::take(active)
}
