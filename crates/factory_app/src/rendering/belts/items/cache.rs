use bevy::prelude::{Entity, Vec3};
use factory_data::ItemId;
use factory_sim::paged_index::PagedIndex;
use factory_sim::{BeltItemId, EntityId};

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
    last_membership_revision: Option<u64>,
    sim_replacement_revision: u64,
    labels_visible: bool,
    aggregate_items: bool,
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

    pub(super) fn membership_changed(&self, revision: u64) -> bool {
        self.last_membership_revision != Some(revision)
    }

    pub(super) fn set_membership_revision(&mut self, revision: u64) {
        self.last_membership_revision = Some(revision);
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

    pub(super) fn aggregate_items(&self) -> bool {
        self.aggregate_items
    }

    pub(super) fn set_aggregate_items(&mut self, aggregate: bool) {
        self.aggregate_items = aggregate;
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

const VACANT_SLOT: u32 = u32::MAX;

/// Sparse presentation index over the shared [`PagedIndex`] page directory.
///
/// The directory holds `u32` entry positions (vacant `u32::MAX`) while
/// occupied values stay contiguous in `entries`; removal uses `swap_remove`
/// with an indirection fix-up, mirroring the simulation caches.
struct SparseSlotMap<I, T> {
    index: PagedIndex<u32>,
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
        self.index.insert(id.raw(), index);
    }

    fn remove(&mut self, id: I) -> Option<T> {
        let entry_index = self.index.remove(id.raw())?;

        let removed = self.entries.swap_remove(entry_index as usize);
        debug_assert!(removed.id == id);
        if (entry_index as usize) < self.entries.len() {
            let moved_id = self.entries[entry_index as usize].id;
            self.index.insert(moved_id.raw(), entry_index);
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
        self.index.clear();
        self.entries.clear();
    }

    fn into_values(self) -> impl Iterator<Item = T> {
        self.entries.into_iter().map(|entry| entry.value)
    }

    fn entry_index(&self, id: I) -> Option<usize> {
        self.index.get(id.raw()).map(|index| index as usize)
    }
}

impl<I, T> Default for SparseSlotMap<I, T> {
    fn default() -> Self {
        Self {
            index: PagedIndex::new(VACANT_SLOT),
            entries: Vec::new(),
        }
    }
}
