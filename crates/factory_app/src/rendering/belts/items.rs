use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::{BasePrototypeIds, ItemId};
#[cfg(test)]
use factory_sim::BELT_SUBTILES_PER_TILE;
use factory_sim::{BeltItemId, EntityId, Simulation};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::constants::BELT_ITEM_SPRITE_SIZE;
#[cfg(test)]
use crate::constants::TILE_SIZE;
use crate::rendering::resources::{
    BeltItemRenderPool, RenderDetail, RenderSyncStats, VisibleEntityIds,
};
use crate::rendering::transforms::tile_translation;
use crate::rendering::visuals::{VisualAssets, spawn_belt_item_visual};
use crate::resources::SimResource;

use super::components::{BeltItemLabel, BeltItemSprite, VisibleBeltItemRenderState};
use super::labels::{belt_item_label, label_translation, spawn_or_reuse_belt_item_label};
#[cfg(test)]
use super::render_state::direction_render_vector;
use super::render_state::{splitter_port_tiles_for_render, transport_item_render_state_from_parts};

#[derive(Default)]
struct BeltItemRenderScratch {
    visible_items: Vec<VisibleBeltItemRenderState>,
    changed_belts: Vec<EntityId>,
    removed_items: Vec<(EntityId, BeltItemId)>,
    interpolated_items: usize,
}

struct CachedBeltItem {
    owner: EntityId,
    item_id: ItemId,
    sprite: Entity,
    label: Option<Entity>,
    previous_translation: Vec3,
    target_translation: Vec3,
    interpolation_frame: u64,
}

#[derive(Default)]
struct CachedBelt {
    revision: u64,
    item_ids: Vec<BeltItemId>,
}

/// Persistent presentation index. Unlike the old frame scratch map, this lets
/// a dirty belt address its own render entities without scanning every item.
#[derive(Default)]
struct BeltItemRenderCache {
    items: SparseSlotMap<BeltItemId, CachedBeltItem>,
    belts: SparseSlotMap<EntityId, CachedBelt>,
    last_item_revision: u64,
    sim_replacement_revision: u64,
    labels_visible: bool,
    interpolation_frame: u64,
}

impl BeltItemRenderCache {
    fn item(&self, item_id: BeltItemId) -> Option<&CachedBeltItem> {
        self.items.get(item_id)
    }

    fn item_mut(&mut self, item_id: BeltItemId) -> Option<&mut CachedBeltItem> {
        self.items.get_mut(item_id)
    }

    fn insert_item(&mut self, item_id: BeltItemId, item: CachedBeltItem) {
        self.items.insert(item_id, item);
    }

    fn remove_item(&mut self, item_id: BeltItemId) -> Option<CachedBeltItem> {
        self.items.remove(item_id)
    }

    fn belt(&self, entity_id: EntityId) -> Option<&CachedBelt> {
        self.belts.get(entity_id)
    }

    fn take_belt(&mut self, entity_id: EntityId) -> Option<CachedBelt> {
        self.belts.remove(entity_id)
    }

    fn insert_belt(&mut self, entity_id: EntityId, belt: CachedBelt) {
        self.belts.insert(entity_id, belt);
    }
}

trait SlotId: Copy + Eq {
    fn raw(self) -> u64;
}

impl SlotId for BeltItemId {
    fn raw(self) -> u64 {
        self.raw()
    }
}

impl SlotId for EntityId {
    fn raw(self) -> u64 {
        self.raw()
    }
}

const SLOT_PAGE_BITS: u32 = 8;
const SLOT_PAGE_SIZE: usize = 1 << SLOT_PAGE_BITS;
const MAX_DIRECT_SLOT_PAGES: usize = 4_096;
const VACANT_SLOT: u32 = u32::MAX;
type SlotPage = [u32; SLOT_PAGE_SIZE];

struct SparseSlotMap<I, T> {
    direct_pages: Vec<Option<Box<SlotPage>>>,
    sparse_pages: HashMap<u64, Box<SlotPage>>,
    entries: Vec<SparseSlotEntry<I, T>>,
}

struct SparseSlotEntry<I, T> {
    id: I,
    value: T,
}

impl<I: SlotId, T> SparseSlotMap<I, T> {
    fn get(&self, id: I) -> Option<&T> {
        let entry = self.entries.get(self.entry_index(id)?)?;
        debug_assert!(entry.id == id);
        Some(&entry.value)
    }

    fn get_mut(&mut self, id: I) -> Option<&mut T> {
        let index = self.entry_index(id)?;
        Some(&mut self.entries.get_mut(index)?.value)
    }

    fn insert(&mut self, id: I, value: T) {
        if let Some(index) = self.entry_index(id) {
            self.entries[index].value = value;
            return;
        }
        let index = u32::try_from(self.entries.len()).expect("belt render cache capacity exceeded");
        self.entries.push(SparseSlotEntry { id, value });
        let (page_id, offset) = slot_location(id);
        self.page_mut_or_insert(page_id)[offset] = index;
    }

    fn remove(&mut self, id: I) -> Option<T> {
        let (page_id, offset) = slot_location(id);
        let page = self.page_mut(page_id)?;
        let entry_index = std::mem::replace(&mut page[offset], VACANT_SLOT);
        if entry_index == VACANT_SLOT {
            return None;
        }
        let page_is_empty = page.iter().all(|index| *index == VACANT_SLOT);
        if page_is_empty {
            self.remove_page(page_id);
        }

        let removed = self.entries.swap_remove(entry_index as usize);
        debug_assert!(removed.id == id);
        if (entry_index as usize) < self.entries.len() {
            let moved_id = self.entries[entry_index as usize].id;
            let (moved_page_id, moved_offset) = slot_location(moved_id);
            self.page_mut(moved_page_id)
                .expect("cached belt item page should exist")[moved_offset] = entry_index;
        }
        Some(removed.value)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn iter(&self) -> impl Iterator<Item = (I, &T)> {
        self.entries.iter().map(|entry| (entry.id, &entry.value))
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = (I, &mut T)> {
        self.entries
            .iter_mut()
            .map(|entry| (entry.id, &mut entry.value))
    }

    fn clear(&mut self) {
        self.direct_pages.clear();
        self.sparse_pages.clear();
        self.entries.clear();
    }

    fn entry_index(&self, id: I) -> Option<usize> {
        let (page_id, offset) = slot_location(id);
        let index = self.page(page_id)?[offset];
        (index != VACANT_SLOT).then_some(index as usize)
    }

    fn page(&self, page_id: u64) -> Option<&SlotPage> {
        if page_id < MAX_DIRECT_SLOT_PAGES as u64 {
            return self.direct_pages.get(page_id as usize)?.as_deref();
        }
        self.sparse_pages.get(&page_id).map(Box::as_ref)
    }

    fn page_mut(&mut self, page_id: u64) -> Option<&mut SlotPage> {
        if page_id < MAX_DIRECT_SLOT_PAGES as u64 {
            return self.direct_pages.get_mut(page_id as usize)?.as_deref_mut();
        }
        self.sparse_pages.get_mut(&page_id).map(Box::as_mut)
    }

    fn page_mut_or_insert(&mut self, page_id: u64) -> &mut SlotPage {
        if page_id < MAX_DIRECT_SLOT_PAGES as u64 {
            let index = page_id as usize;
            if self.direct_pages.len() <= index {
                self.direct_pages.resize_with(index + 1, || None);
            }
            return self.direct_pages[index]
                .get_or_insert_with(|| Box::new([VACANT_SLOT; SLOT_PAGE_SIZE]));
        }
        self.sparse_pages
            .entry(page_id)
            .or_insert_with(|| Box::new([VACANT_SLOT; SLOT_PAGE_SIZE]))
    }

    fn remove_page(&mut self, page_id: u64) {
        if page_id < MAX_DIRECT_SLOT_PAGES as u64 {
            self.direct_pages[page_id as usize] = None;
        } else {
            self.sparse_pages.remove(&page_id);
        }
    }
}

impl<I, T> Default for SparseSlotMap<I, T> {
    fn default() -> Self {
        Self {
            direct_pages: Vec::new(),
            sparse_pages: HashMap::new(),
            entries: Vec::new(),
        }
    }
}

fn slot_location<I: SlotId>(id: I) -> (u64, usize) {
    let raw = id.raw();
    (
        raw >> SLOT_PAGE_BITS,
        (raw & (SLOT_PAGE_SIZE as u64 - 1)) as usize,
    )
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
        if detail.is_changed() && !cache.items.is_empty() {
            pool_cached_belt_items(&mut cache, &mut pool, &mut sprites, &mut labels);
        }
        return;
    }

    let sim_replaced = cache.sim_replacement_revision != sim.replacement_revision();
    if sim_replaced {
        cache.sim_replacement_revision = sim.replacement_revision();
        pool_cached_belt_items(&mut cache, &mut pool, &mut sprites, &mut labels);
    }
    let alpha = fixed_time
        .as_deref()
        .map_or(1.0, Time::<Fixed>::overstep_fraction)
        .clamp(0.0, 1.0);
    cache.interpolation_frame = cache.interpolation_frame.wrapping_add(1).max(1);
    let interpolation_frame = cache.interpolation_frame;
    scratch.interpolated_items = 0;
    let sim = sim.read();
    let ids = BasePrototypeIds::from_catalog(sim.catalog());
    let visibility_changed = visible_entity_ids.is_changed() || detail.is_changed() || sim_replaced;
    let items_changed = cache.last_item_revision != sim.belt_item_revision();
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
        cache.last_item_revision = sim.belt_item_revision();
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
            cache.belts.iter().filter_map(|(entity_id, _)| {
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

    if cache.labels_visible != show_labels {
        sync_label_visibility(commands, sim, show_labels, pool, cache, labels);
        cache.labels_visible = show_labels;
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
        if let Some(cached) = cache.item_mut(item.key) {
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
            if let Ok((_, mut marker, mut transform, mut sprite, _)) =
                sprites.get_mut(cached.sprite)
            {
                transform.translation = translation;
                if item_type_changed {
                    marker.item_id = item.item_id;
                    *sprite = visual_assets
                        .belt_item_sprite(item.color, Vec2::splat(BELT_ITEM_SPRITE_SIZE));
                }
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
            continue;
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
    if already_interpolated == cache.items.len() {
        return;
    }
    for (key, item) in cache.items.iter_mut() {
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
    for (key, item) in cache.items.iter_mut() {
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
        } else if !show_labels && let Some(entity) = item.label.take() {
            if let Ok((_, mut marker, _, _, mut visibility)) = labels.get_mut(entity) {
                marker.active = false;
                *visibility = Visibility::Hidden;
            }
            push_unique(&mut pool.labels, entity);
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
    let items = std::mem::take(&mut cache.items);
    for entry in items.entries {
        pool_cached_item(entry.value, pool, sprites, labels);
    }
    cache.belts.clear();
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
    if let Ok((_, mut marker, _, _, mut visibility)) = sprites.get_mut(item.sprite) {
        marker.active = false;
        *visibility = Visibility::Hidden;
    }
    push_unique(&mut pool.sprites, item.sprite);
    if let Some(label) = item.label {
        if let Ok((_, mut marker, _, _, mut visibility)) = labels.get_mut(label) {
            marker.active = false;
            *visibility = Visibility::Hidden;
        }
        push_unique(&mut pool.labels, label);
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

#[cfg(test)]
pub(super) fn collect_visible_belt_items_into(
    sim: &Simulation,
    ids: BasePrototypeIds,
    visible_ids: &HashSet<EntityId>,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    items.clear();
    for &entity_id in visible_ids {
        collect_belt_items_append(sim, ids, entity_id, items);
    }
}

fn collect_belt_items_into(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    items.clear();
    collect_belt_items_append(sim, ids, entity_id, items);
}

fn collect_belt_items_append(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    items: &mut Vec<VisibleBeltItemRenderState>,
) {
    let Some(placed) = sim.entities().placed_entity(entity_id) else {
        return;
    };
    if let Ok(segment) = factory_sim::entity_access::belt_segment(sim, placed.id) {
        let center = tile_translation(placed.x, placed.y, 4.0);
        for (lane_index, lane) in segment.lanes.iter().enumerate() {
            for item in &lane.items {
                items.push(transport_item_render_state_from_parts(
                    item.id,
                    lane_index,
                    segment.dir,
                    center,
                    item.item_id,
                    item.position_subtile,
                    belt_item_color(item.item_id, ids),
                ));
            }
        }
        return;
    }

    let Ok(state) = factory_sim::entity_access::splitter_state(sim, placed.id) else {
        return;
    };
    let Some(port_tiles) = splitter_port_tiles_for_render(&placed.footprint) else {
        return;
    };
    for (input_port, input_lanes) in state.input_lanes.iter().enumerate() {
        let port_tile = port_tiles[input_port];
        let center = tile_translation(port_tile.0, port_tile.1, 4.0);
        for (lane_index, lane) in input_lanes.iter().enumerate() {
            for item in &lane.items {
                items.push(transport_item_render_state_from_parts(
                    item.id,
                    lane_index,
                    state.dir,
                    center,
                    item.item_id,
                    item.position_subtile,
                    belt_item_color(item.item_id, ids),
                ));
            }
        }
    }
}

pub(super) fn push_unique(pool: &mut Vec<Entity>, entity: Entity) {
    if !pool.contains(&entity) {
        pool.push(entity);
    }
}

#[cfg(test)]
pub(crate) fn belt_item_render_state(
    sim: &Simulation,
    entity_id: EntityId,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, Color)> {
    transport_item_render_state_with_ids(
        sim,
        BasePrototypeIds::from_catalog(sim.catalog()),
        entity_id,
        None,
        lane_index,
        item_index,
    )
}

#[cfg(test)]
pub(super) fn transport_item_render_state_with_ids(
    sim: &Simulation,
    ids: BasePrototypeIds,
    entity_id: EntityId,
    input_port: Option<usize>,
    lane_index: usize,
    item_index: usize,
) -> Option<(Vec3, Color)> {
    let placed = sim.entities().placed_entity(entity_id)?;
    let (dir, item, center) = if let Some(input_port) = input_port {
        let state = factory_sim::entity_access::splitter_state(sim, entity_id).ok()?;
        let item = state
            .input_lanes
            .get(input_port)?
            .get(lane_index)?
            .items
            .get(item_index)?;
        let port_tile = splitter_port_tiles_for_render(&placed.footprint)?[input_port];
        (
            state.dir,
            item,
            tile_translation(port_tile.0, port_tile.1, 4.0),
        )
    } else {
        let segment = factory_sim::entity_access::belt_segment(sim, entity_id).ok()?;
        let item = segment.lanes.get(lane_index)?.items.get(item_index)?;
        (segment.dir, item, tile_translation(placed.x, placed.y, 4.0))
    };
    let along = direction_render_vector(dir);
    let perpendicular = Vec2::new(-along.y, along.x);
    let progress = f32::from(item.position_subtile) / f32::from(BELT_SUBTILES_PER_TILE) - 0.5;
    let lane_offset = if lane_index == 0 { -0.18 } else { 0.18 };
    let offset = (along * progress + perpendicular * lane_offset) * TILE_SIZE;
    let color = belt_item_color(item.item_id, ids);

    Some((
        Vec3::new(center.x + offset.x, center.y + offset.y, 4.0),
        color,
    ))
}

pub(crate) fn belt_item_color(item_id: ItemId, ids: BasePrototypeIds) -> Color {
    if item_id == ids.items.iron_ore {
        Color::srgb(0.72, 0.67, 0.58)
    } else if item_id == ids.items.copper_ore {
        Color::srgb(0.86, 0.42, 0.20)
    } else if item_id == ids.items.coal {
        Color::srgb(0.055, 0.055, 0.052)
    } else if item_id == ids.items.stone {
        Color::srgb(0.54, 0.51, 0.47)
    } else if item_id == ids.items.iron_plate || item_id == ids.items.steel_plate {
        Color::srgb(0.78, 0.82, 0.84)
    } else if item_id == ids.items.copper_plate || item_id == ids.items.copper_cable {
        Color::srgb(0.92, 0.54, 0.24)
    } else if item_id == ids.items.iron_gear_wheel {
        Color::srgb(0.66, 0.70, 0.74)
    } else if item_id == ids.items.electronic_circuit {
        Color::srgb(0.24, 0.70, 0.38)
    } else if item_id == ids.items.automation_science_pack {
        Color::srgb(0.88, 0.24, 0.20)
    } else if item_id == ids.items.logistic_science_pack {
        Color::srgb(0.32, 0.72, 0.36)
    } else {
        Color::srgb(0.58, 0.78, 0.94)
    }
}
