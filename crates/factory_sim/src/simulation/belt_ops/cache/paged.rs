//! Sparse paged index storage for belt-transport runtime caches.
//!
//! Entity IDs are allocated monotonically and never reused, so a vector
//! indexed directly by raw entity ID grows with the highest ID ever issued,
//! even after those entities are destroyed. The maps here store fixed-size
//! pages of 256 entries addressed by `index >> 8`: low pages live in a direct
//! vector for single-indirection hot-path lookup, while high pages live in a
//! hash map so sparse high IDs allocate only their own pages. A page freed by
//! incremental entity removal is dropped, so repeated build/destroy churn
//! settles at a stable footprint instead of tracking the ID high-water mark.
//! Full rebuilds park live pages in a pool that is dropped when the rebuild
//! completes, so a shrinking factory releases its peak footprint.
//!
//! The paging layout mirrors the belt render cache's `SparseSlotMap` and
//! [`DenseEntityMap`]: the pages hold plain copy values directly instead of
//! dense entry records because transport lookups only need one integer per
//! index. Unifying the three page-directory implementations is tracked as a
//! separate refactor in #335; it is out of scope here because the value
//! shapes, crate homes, and serialization constraints differ.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::simulation::{EntityId, EntityStore};

/// Entries per page. Pages hold one value per index, so a page of `u32` slots
/// costs 1 KiB and a page of `u64` revisions costs 2 KiB.
const PAGE_BITS: u32 = 8;
/// Number of values per page.
const PAGE_SIZE: usize = 1 << PAGE_BITS;
/// Direct pages below this count live in a vector; higher pages live in a
/// hash map so sparse high indices allocate only their own pages.
const MAX_DIRECT_PAGES: usize = 4_096;
/// Conservative heap estimate per sparse-map bucket: key, value pointer, and
/// table overhead. Used only by test storage accounting.
#[cfg(test)]
const SPARSE_MAP_BYTES_PER_BUCKET: usize = 8 + 8 + 8;

/// Sentinel stored for unmapped lane slots. Graph call sites never observe
/// it: [`PagedSparseVec`] filters vacant entries at its boundary and reports
/// vacancy as `None`.
pub(in crate::simulation::belt_ops) const VACANT_SLOT: u32 = u32::MAX;

/// Splits a raw index into its page id and offset within the page.
fn split_index(index: u64) -> (u64, usize) {
    (
        index >> PAGE_BITS,
        (index & (PAGE_SIZE as u64 - 1)) as usize,
    )
}

/// Fixed-size paged vector keyed by a dense `u64` index with a vacant default.
///
/// Missing pages read as vacant and are only allocated on write, so sparse
/// high indices cost their own pages rather than every index below them.
/// Vacancy is represented as `None` at every public boundary; the vacant
/// value itself is never stored.
#[derive(Clone)]
struct PagedSparseVec<T>
where
    T: Copy + PartialEq,
{
    vacant: T,
    direct_pages: Vec<Option<Box<[T; PAGE_SIZE]>>>,
    sparse_pages: HashMap<u64, Box<[T; PAGE_SIZE]>>,
    /// Empty boxes parked by [`PagedSparseVec::begin_rebuild`] for reuse
    /// during the rebuild. The pool is strictly rebuild-scoped: incremental
    /// removals drop their boxes instead of pooling them, and
    /// [`PagedSparseVec::end_rebuild`] drops whatever the rebuild did not
    /// reuse, so a shrinking factory releases its peak footprint instead of
    /// retaining it.
    page_pool: Vec<Box<[T; PAGE_SIZE]>>,
}

impl<T> PagedSparseVec<T>
where
    T: Copy + PartialEq,
{
    /// Creates an empty map whose missing entries read as `vacant`.
    fn new(vacant: T) -> Self {
        Self {
            vacant,
            direct_pages: Vec::new(),
            sparse_pages: HashMap::new(),
            page_pool: Vec::new(),
        }
    }

    /// Returns the stored value, or `None` when the index is vacant.
    fn get(&self, index: u64) -> Option<T> {
        let value = self.page(index)?[split_index(index).1];
        (value != self.vacant).then_some(value)
    }

    /// Stores `value`, allocating its page on first write. A vacant value
    /// removes the entry instead so the vacant sentinel is never stored.
    fn insert(&mut self, index: u64, value: T) {
        let vacant = self.vacant;
        debug_assert!(
            value != vacant,
            "paged transport maps never store the vacant value"
        );
        if value == vacant {
            self.remove(index);
            return;
        }
        let (page_id, offset) = split_index(index);
        self.page_mut_or_insert(page_id)[offset] = value;
    }

    /// Clears one entry, freeing its page when the page becomes empty.
    /// Returns the previous value, or `None` when already vacant.
    fn remove(&mut self, index: u64) -> Option<T> {
        let vacant = self.vacant;
        let (page_id, offset) = split_index(index);
        let previous = {
            let page = self.page_mut(page_id)?;
            let previous = page[offset];
            if previous == vacant {
                return None;
            }
            page[offset] = vacant;
            if page.iter().any(|value| *value != vacant) {
                return Some(previous);
            }
            previous
        };
        self.remove_page(page_id);
        Some(previous)
    }

    /// Parks every live page for reuse by an imminent full rebuild, dropping
    /// all entries. Directory buffers are retained, mirroring how the
    /// previous dense vector kept its backing allocation. Must be paired
    /// with [`PagedSparseVec::end_rebuild`].
    fn begin_rebuild(&mut self) {
        debug_assert!(
            self.page_pool.is_empty(),
            "transport rebuilds must not nest"
        );
        self.page_pool
            .extend(self.direct_pages.iter_mut().filter_map(|slot| slot.take()));
        self.direct_pages.clear();
        self.page_pool
            .extend(self.sparse_pages.drain().map(|(_, page)| page));
    }

    /// Drops pooled pages the rebuild did not reuse, so the pool never
    /// outlives the rebuild it served. Call once the rebuild has assigned
    /// every live entry.
    fn end_rebuild(&mut self) {
        self.page_pool = Vec::new();
    }

    /// Drops every occupied entry whose raw index is rejected by `keep`,
    /// freeing pages left empty. Removal order does not affect the outcome,
    /// so hash-map iteration order cannot leak into simulation behavior.
    fn retain(&mut self, mut keep: impl FnMut(u64) -> bool) {
        for (page_number, slot) in self.direct_pages.iter_mut().enumerate() {
            let Some(page) = slot else {
                continue;
            };
            let base = (page_number as u64) << PAGE_BITS;
            let mut page_live = false;
            for (offset, value) in page.iter_mut().enumerate() {
                if *value == self.vacant {
                    continue;
                }
                if keep(base + offset as u64) {
                    page_live = true;
                } else {
                    *value = self.vacant;
                }
            }
            if !page_live {
                *slot = None;
            }
        }
        let vacant = self.vacant;
        let mut emptied = Vec::new();
        for (&page_id, page) in self.sparse_pages.iter_mut() {
            let base = page_id << PAGE_BITS;
            let mut page_live = false;
            for (offset, value) in page.iter_mut().enumerate() {
                if *value == vacant {
                    continue;
                }
                if keep(base + offset as u64) {
                    page_live = true;
                } else {
                    *value = vacant;
                }
            }
            if !page_live {
                emptied.push(page_id);
            }
        }
        for page_id in emptied {
            self.sparse_pages.remove(&page_id);
        }
    }

    /// Returns the page holding `index`, or `None` when never allocated.
    fn page(&self, index: u64) -> Option<&[T; PAGE_SIZE]> {
        let (page_id, _) = split_index(index);
        if page_id < MAX_DIRECT_PAGES as u64 {
            return self
                .direct_pages
                .get(page_id as usize)?
                .as_ref()
                .map(Box::as_ref);
        }
        self.sparse_pages.get(&page_id).map(Box::as_ref)
    }

    /// Returns the allocated page, or `None` when never allocated.
    fn page_mut(&mut self, page_id: u64) -> Option<&mut [T; PAGE_SIZE]> {
        if page_id < MAX_DIRECT_PAGES as u64 {
            return self
                .direct_pages
                .get_mut(page_id as usize)?
                .as_mut()
                .map(Box::as_mut);
        }
        self.sparse_pages.get_mut(&page_id).map(Box::as_mut)
    }

    /// Returns the page, reusing a retained box when one is pooled and
    /// allocating otherwise.
    fn page_mut_or_insert(&mut self, page_id: u64) -> &mut [T; PAGE_SIZE] {
        if page_id < MAX_DIRECT_PAGES as u64 {
            let index = page_id as usize;
            if self.direct_pages.len() <= index {
                self.direct_pages.resize_with(index + 1, || None);
            }
            if self.direct_pages[index].is_none() {
                let page = self.pooled_page();
                self.direct_pages[index] = Some(page);
            }
            return self.direct_pages[index]
                .as_mut()
                .map(Box::as_mut)
                .expect("direct transport page should exist after pooling");
        }
        if !self.sparse_pages.contains_key(&page_id) {
            let page = self.pooled_page();
            self.sparse_pages.insert(page_id, page);
        }
        self.sparse_pages
            .get_mut(&page_id)
            .map(Box::as_mut)
            .expect("sparse transport page should exist after pooling")
    }

    /// Pops a retained box, resetting it to vacant, or allocates a fresh one.
    /// Pooled boxes may hold stale values, so every reuse refills them.
    fn pooled_page(&mut self) -> Box<[T; PAGE_SIZE]> {
        if let Some(mut page) = self.page_pool.pop() {
            page.fill(self.vacant);
            page
        } else {
            Box::new([self.vacant; PAGE_SIZE])
        }
    }

    /// Drops one empty page. Incremental removals free their boxes instead
    /// of pooling them so churn cannot accumulate retained pages.
    fn remove_page(&mut self, page_id: u64) {
        if page_id < MAX_DIRECT_PAGES as u64 {
            self.direct_pages[page_id as usize] = None;
        } else {
            self.sparse_pages.remove(&page_id);
        }
    }

    /// Occupied entries in ascending index order. Used only for equality,
    /// hashing, and debug output, so the allocation never touches hot paths.
    fn occupied_entries(&self) -> Vec<(u64, T)> {
        let mut entries = Vec::new();
        for (page_number, slot) in self.direct_pages.iter().enumerate() {
            let Some(page) = slot else {
                continue;
            };
            let base = (page_number as u64) << PAGE_BITS;
            for (offset, &value) in page.iter().enumerate() {
                if value != self.vacant {
                    entries.push((base + offset as u64, value));
                }
            }
        }
        let mut sparse_ids: Vec<u64> = self.sparse_pages.keys().copied().collect();
        sparse_ids.sort_unstable();
        for page_id in sparse_ids {
            let base = page_id << PAGE_BITS;
            for (offset, &value) in self.sparse_pages[&page_id].iter().enumerate() {
                if value != self.vacant {
                    entries.push((base + offset as u64, value));
                }
            }
        }
        entries
    }

    /// Counts allocated (non-pooled) value pages.
    #[cfg(test)]
    fn allocated_pages(&self) -> usize {
        self.direct_pages
            .iter()
            .filter(|slot| slot.is_some())
            .count()
            + self.sparse_pages.len()
    }

    /// Estimates all heap held for this index: live and pooled value pages
    /// plus the page-directory buffers (direct vector, sparse-map buckets,
    /// and pool vector). Sparse buckets use a conservative per-bucket
    /// estimate, so the total may slightly overstate rather than understate.
    #[cfg(test)]
    fn storage_bytes(&self) -> usize {
        let payload =
            (self.allocated_pages() + self.page_pool.len()) * PAGE_SIZE * std::mem::size_of::<T>();
        let directory = self.direct_pages.capacity()
            * std::mem::size_of::<Option<Box<[T; PAGE_SIZE]>>>()
            + self.page_pool.capacity() * std::mem::size_of::<Box<[T; PAGE_SIZE]>>()
            + self
                .sparse_pages
                .capacity()
                .saturating_mul(SPARSE_MAP_BYTES_PER_BUCKET);
        payload + directory
    }
}

impl<T> PartialEq for PagedSparseVec<T>
where
    T: Copy + PartialEq,
{
    /// Compares logical contents only: pooled boxes and page layout do not
    /// affect equality.
    fn eq(&self, other: &Self) -> bool {
        self.vacant == other.vacant && self.occupied_entries() == other.occupied_entries()
    }
}

impl<T> Eq for PagedSparseVec<T> where T: Copy + PartialEq + Eq {}

impl<T> Hash for PagedSparseVec<T>
where
    T: Copy + PartialEq + Hash,
{
    /// Hashes the logical contents in index order so equal maps hash
    /// equally regardless of page layout, pool contents, or hash-map
    /// iteration order.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vacant.hash(state);
        for (index, value) in self.occupied_entries() {
            index.hash(state);
            value.hash(state);
        }
    }
}

impl<T> std::fmt::Debug for PagedSparseVec<T>
where
    T: Copy + PartialEq + std::fmt::Debug,
{
    /// Prints logical contents only; pooled boxes are omitted.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagedSparseVec")
            .field("vacant", &self.vacant)
            .field("entries", &self.occupied_entries())
            .finish()
    }
}

/// Maps `entity_id * 4 + lane_offset` wakeup keys onto compact lane slots.
///
/// Pages are allocated for live transport entities only; removing an
/// entity's last lane frees its page when nothing else shares it, and full
/// rebuilds reuse parked pages, dropping the unneeded remainder when done.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneSlotMap {
    inner: PagedSparseVec<u32>,
}

impl TransportLaneSlotMap {
    /// Returns the slot for a wakeup key, or `None` when unmapped.
    pub(in crate::simulation) fn get(&self, raw: usize) -> Option<u32> {
        self.inner.get(raw as u64)
    }

    /// Maps a wakeup key onto a slot, allocating its page on first write.
    pub(in crate::simulation) fn insert(&mut self, raw: usize, slot: u32) {
        self.inner.insert(raw as u64, slot);
    }

    /// Unmaps a wakeup key, freeing its page when left empty. Returns the
    /// previous slot, or `None` when already unmapped.
    pub(in crate::simulation) fn remove(&mut self, raw: usize) -> Option<u32> {
        self.inner.remove(raw as u64)
    }

    /// Parks every mapping for reuse by an imminent full rebuild. Must be
    /// paired with [`TransportLaneSlotMap::end_rebuild`].
    pub(in crate::simulation) fn begin_rebuild(&mut self) {
        self.inner.begin_rebuild();
    }

    /// Drops parked pages the rebuild did not reuse. The pool never outlives
    /// the rebuild it served.
    pub(in crate::simulation) fn end_rebuild(&mut self) {
        self.inner.end_rebuild();
    }

    /// Estimates all heap held for the slot index; see
    /// [`PagedSparseVec::storage_bytes`].
    #[cfg(test)]
    pub(in crate::simulation) fn storage_bytes(&self) -> usize {
        self.inner.storage_bytes()
    }
}

impl Default for PagedSparseVec<u32> {
    /// Defaults to the lane-slot vacant sentinel.
    fn default() -> Self {
        Self::new(VACANT_SLOT)
    }
}

/// Per-entity belt-item change tokens consumed by incremental presentation.
///
/// Only transport entities that changed items carry a token; tokens for
/// destroyed entities are pruned on the next topology refresh so storage
/// scales with live transport state instead of the ID high-water mark.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct EntityItemRevisionMap {
    inner: PagedSparseVec<u64>,
}

impl EntityItemRevisionMap {
    /// Returns the entity's change token, or `0` when it never changed items.
    pub(in crate::simulation) fn revision(&self, entity_id: EntityId) -> u64 {
        self.inner.get(entity_id.raw()).unwrap_or(0)
    }

    /// Records a change token for a live transport entity.
    pub(in crate::simulation) fn set(&mut self, entity_id: EntityId, revision: u64) {
        self.inner.insert(entity_id.raw(), revision);
    }

    /// Drops one entity's token, freeing its page when left empty.
    pub(in crate::simulation) fn remove(&mut self, entity_id: EntityId) {
        self.inner.remove(entity_id.raw());
    }

    /// Drops tokens for entities that are no longer belt/splitter transport
    /// state. All producers mark transport entities only, so pruning to those
    /// maps cannot drop a live token.
    pub(in crate::simulation) fn retain_live_transport(&mut self, entities: &EntityStore) {
        self.inner.retain(|raw| {
            let id = EntityId::new(raw);
            entities.transport_belts.contains_key(&id) || entities.splitters.contains_key(&id)
        });
    }

    /// Estimates all heap held for the revision index; see
    /// [`PagedSparseVec::storage_bytes`].
    #[cfg(test)]
    pub(in crate::simulation) fn storage_bytes(&self) -> usize {
        self.inner.storage_bytes()
    }
}

impl Default for PagedSparseVec<u64> {
    /// Defaults to a zero vacant token, matching "never changed".
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// High indices allocate only their own pages, not the range below them.
    #[test]
    fn sparse_high_indices_allocate_only_their_pages() {
        let mut map = TransportLaneSlotMap::default();
        map.insert(3, 7);
        map.insert(10_000_000 * 4 + 1, 9);

        assert_eq!(map.get(3), Some(7));
        assert_eq!(map.get(10_000_000 * 4 + 1), Some(9));
        assert_eq!(map.get(4), None);
        assert_eq!(map.inner.allocated_pages(), 2);
        // Two page payloads plus small directory buffers.
        assert!(map.storage_bytes() >= 2 * PAGE_SIZE * 4);
        assert!(map.storage_bytes() < 2 * PAGE_SIZE * 4 + 4096);
    }

    /// Removing the last entry of a page frees the page itself.
    #[test]
    fn removing_last_entry_frees_its_page() {
        let mut map = TransportLaneSlotMap::default();
        let high = u64::from(MAX_DIRECT_PAGES as u32) * PAGE_SIZE as u64 + 5;
        map.insert(0, 1);
        map.insert(high as usize, 2);
        assert_eq!(map.inner.allocated_pages(), 2);

        assert_eq!(map.remove(high as usize), Some(2));
        assert_eq!(map.get(high as usize), None);
        assert_eq!(map.inner.allocated_pages(), 1);

        assert_eq!(map.remove(0), Some(1));
        assert_eq!(map.inner.allocated_pages(), 0);
        assert_eq!(map, TransportLaneSlotMap::default());
    }

    /// `retain` drops rejected indices and frees pages left empty, including
    /// sparse hash-backed pages.
    #[test]
    fn revision_retain_drops_rejected_indices() {
        let mut map = EntityItemRevisionMap::default();
        map.set(EntityId::new(7), 3);
        map.set(EntityId::new(261), 5);
        map.set(EntityId::new(1_100_000), 9);

        map.inner.retain(|raw| raw != 261);

        assert_eq!(map.revision(EntityId::new(7)), 3);
        assert_eq!(map.revision(EntityId::new(261)), 0);
        assert_eq!(map.revision(EntityId::new(1_100_000)), 9);
        // The surviving low page stays allocated while the emptied pages are
        // gone entirely.
        assert_eq!(map.inner.allocated_pages(), 2);
    }

    /// A rebuild parks live pages, reuses the ones it still needs, and drops
    /// the unneeded remainder when it completes, so the pool never outlives
    /// the rebuild it served.
    #[test]
    fn rebuild_scope_reuses_pages_then_drops_remainder() {
        let mut map = TransportLaneSlotMap::default();
        map.insert(3, 7);
        map.insert(900_000, 9);
        assert_eq!(map.inner.allocated_pages(), 2);

        map.begin_rebuild();
        assert_eq!(map.get(3), None);
        assert_eq!(map.inner.allocated_pages(), 0);
        assert_eq!(map.inner.page_pool.len(), 2);

        map.insert(300, 11);
        assert_eq!(map.get(300), Some(11));
        assert_eq!(map.inner.page_pool.len(), 1);
        assert_eq!(map.inner.allocated_pages(), 1);

        map.end_rebuild();
        assert_eq!(map.inner.page_pool.len(), 0);
        assert_eq!(map.get(300), Some(11));
    }

    /// Equal contents compare and hash equally regardless of page layout or
    /// pooled boxes.
    #[test]
    fn equal_contents_compare_equal_regardless_of_page_layout() {
        use std::collections::hash_map::DefaultHasher;

        let mut first = TransportLaneSlotMap::default();
        first.insert(1, 10);
        first.insert(900_000, 20);
        first.remove(1);
        first.insert(1, 10);

        let mut second = TransportLaneSlotMap::default();
        second.insert(900_000, 20);
        second.insert(1, 10);

        assert_eq!(first, second);
        let hash = |map: &TransportLaneSlotMap| {
            let mut hasher = DefaultHasher::new();
            map.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(&first), hash(&second));
    }
}
