use bevy::prelude::{Entity, Vec3};
use factory_data::ItemId;
use factory_sim::{BeltItemId, EntityId};
use std::collections::HashMap;

pub(super) struct CachedBeltItem {
    pub(super) owner: EntityId,
    pub(super) item_id: ItemId,
    pub(super) sprite: Entity,
    pub(super) label: Option<Entity>,
    pub(super) previous_translation: Vec3,
    pub(super) target_translation: Vec3,
    pub(super) interpolation_frame: u64,
}

#[derive(Default)]
pub(super) struct CachedBelt {
    pub(super) revision: u64,
    pub(super) item_ids: Vec<BeltItemId>,
}

/// Persistent presentation index. Unlike the old frame scratch map, this lets
/// a dirty belt address its own render entities without scanning every item.
#[derive(Default)]
pub(super) struct BeltItemRenderCache {
    items: SparseSlotMap<BeltItemId, CachedBeltItem>,
    belts: SparseSlotMap<EntityId, CachedBelt>,
    last_item_revision: u64,
    sim_replacement_revision: u64,
    labels_visible: bool,
    interpolation_frame: u64,
}

impl BeltItemRenderCache {
    pub(super) fn has_items(&self) -> bool {
        !self.items.is_empty()
    }

    pub(super) fn item_count(&self) -> usize {
        self.items.len()
    }

    pub(super) fn item(&self, item_id: BeltItemId) -> Option<&CachedBeltItem> {
        self.items.get(item_id)
    }

    pub(super) fn item_mut(&mut self, item_id: BeltItemId) -> Option<&mut CachedBeltItem> {
        self.items.get_mut(item_id)
    }

    pub(super) fn insert_item(&mut self, item_id: BeltItemId, item: CachedBeltItem) {
        self.items.insert(item_id, item);
    }

    pub(super) fn remove_item(&mut self, item_id: BeltItemId) -> Option<CachedBeltItem> {
        self.items.remove(item_id)
    }

    pub(super) fn items_mut(&mut self) -> impl Iterator<Item = (BeltItemId, &mut CachedBeltItem)> {
        self.items.iter_mut()
    }

    pub(super) fn take_items(&mut self) -> impl Iterator<Item = CachedBeltItem> + use<> + 'static {
        std::mem::take(&mut self.items).into_values()
    }

    pub(super) fn belt(&self, entity_id: EntityId) -> Option<&CachedBelt> {
        self.belts.get(entity_id)
    }

    pub(super) fn take_belt(&mut self, entity_id: EntityId) -> Option<CachedBelt> {
        self.belts.remove(entity_id)
    }

    pub(super) fn insert_belt(&mut self, entity_id: EntityId, belt: CachedBelt) {
        self.belts.insert(entity_id, belt);
    }

    pub(super) fn belts(&self) -> impl Iterator<Item = (EntityId, &CachedBelt)> {
        self.belts.iter()
    }

    pub(super) fn clear_belts(&mut self) {
        self.belts.clear();
    }

    pub(super) fn last_item_revision(&self) -> u64 {
        self.last_item_revision
    }

    pub(super) fn set_last_item_revision(&mut self, revision: u64) {
        self.last_item_revision = revision;
    }

    pub(super) fn sim_replacement_revision(&self) -> u64 {
        self.sim_replacement_revision
    }

    pub(super) fn set_sim_replacement_revision(&mut self, revision: u64) {
        self.sim_replacement_revision = revision;
    }

    pub(super) fn labels_visible(&self) -> bool {
        self.labels_visible
    }

    pub(super) fn set_labels_visible(&mut self, visible: bool) {
        self.labels_visible = visible;
    }

    pub(super) fn advance_interpolation_frame(&mut self) -> u64 {
        self.interpolation_frame = self.interpolation_frame.wrapping_add(1).max(1);
        self.interpolation_frame
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

    fn into_values(self) -> impl Iterator<Item = T> {
        self.entries.into_iter().map(|entry| entry.value)
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
