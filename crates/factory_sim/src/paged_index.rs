//! Shared paged index for entity-keyed caches.
//!
//! Three caches share the same paging shape: 256-entry pages addressed by
//! `index >> 8`, a direct page vector for low pages with a ~4096-page cutoff,
//! sparse hash-backed pages for high indices, and an allocate-on-write /
//! free-when-empty lifecycle:
//!
//! * [`DenseEntityMap`](crate::entities) indirection pages,
//! * belt render cache `SparseSlotMap`,
//! * transport [`PagedIndex`]-backed maps (lane slots, item revisions).
//!
//! This module owns page allocation, lookup, freeing, and capacity
//! accounting. Index-to-entry maps (dense `entries` vectors with swap-remove)
//! stay with their owners so iteration order, serialization, and density
//! guarantees do not change. Pages hold plain `Copy` values directly; index
//! maps store `u32` entry positions with `u32::MAX` as the vacant sentinel,
//! while transport maps store `u32`/`u64` payloads with their own sentinels.
//!
//! Hot-path lookup is a single indirection for low pages: one vector index
//! plus one array offset. High pages add one hash lookup. Pages are only
//! allocated on write, and a page freed by removal is dropped, so churned or
//! sparse high indices never inflate the footprint. Full rebuilds may park
//! live pages in a rebuild-scoped pool that is dropped when the rebuild
//! completes.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Bits addressing one page: page id is `index >> PAGE_BITS`.
pub const PAGE_BITS: u32 = 8;
/// Number of values per page.
pub const PAGE_SIZE: usize = 1 << PAGE_BITS;
/// Direct pages below this count live in a vector; higher pages live in a
/// hash map so sparse high indices allocate only their own pages.
pub const MAX_DIRECT_PAGES: usize = 4_096;
/// Conservative heap estimate per sparse-map bucket: key, value pointer, and
/// table overhead. Used only by test storage accounting.
const SPARSE_MAP_BYTES_PER_BUCKET: usize = 8 + 8 + 8;

/// Splits a raw index into its page id and offset within the page.
#[inline]
pub fn split_index(index: u64) -> (u64, usize) {
    (
        index >> PAGE_BITS,
        (index & (PAGE_SIZE as u64 - 1)) as usize,
    )
}

/// Fixed-size paged index keyed by a dense `u64` with a vacant default.
///
/// Missing pages read as vacant and are only allocated on write, so sparse
/// high indices cost their own pages rather than every index below them.
/// Vacancy is represented as `None` at every public boundary; the vacant
/// value itself is never stored.
#[derive(Clone)]
pub struct PagedIndex<T>
where
    T: Copy + PartialEq,
{
    vacant: T,
    direct_pages: Vec<Option<Box<[T; PAGE_SIZE]>>>,
    sparse_pages: HashMap<u64, Box<[T; PAGE_SIZE]>>,
    /// Empty boxes parked by [`PagedIndex::begin_rebuild`] for reuse
    /// during the rebuild. The pool is strictly rebuild-scoped: incremental
    /// removals drop their boxes instead of pooling them, and
    /// [`PagedIndex::end_rebuild`] drops whatever the rebuild did not
    /// reuse, so a shrinking population releases its peak footprint instead
    /// of retaining it.
    page_pool: Vec<Box<[T; PAGE_SIZE]>>,
}

impl<T> PagedIndex<T>
where
    T: Copy + PartialEq,
{
    /// Creates an empty index whose missing entries read as `vacant`.
    pub fn new(vacant: T) -> Self {
        Self {
            vacant,
            direct_pages: Vec::new(),
            sparse_pages: HashMap::new(),
            page_pool: Vec::new(),
        }
    }

    /// Returns the vacant sentinel for this index.
    #[inline]
    pub fn vacant(&self) -> T {
        self.vacant
    }

    /// Returns the stored value, or `None` when the index is vacant.
    #[inline]
    pub fn get(&self, index: u64) -> Option<T> {
        let value = self.page(index)?[split_index(index).1];
        (value != self.vacant).then_some(value)
    }

    /// Returns `true` when the index holds a non-vacant value.
    #[inline]
    pub fn contains(&self, index: u64) -> bool {
        self.get(index).is_some()
    }

    /// Stores `value`, allocating its page on first write. A vacant value
    /// removes the entry instead so the vacant sentinel is never stored.
    pub fn insert(&mut self, index: u64, value: T) {
        let vacant = self.vacant;
        debug_assert!(
            value != vacant,
            "paged indexes never store the vacant value"
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
    pub fn remove(&mut self, index: u64) -> Option<T> {
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

    /// Drops every entry and pooled page. Directory buffers keep their
    /// capacity, mirroring `Vec::clear` / `HashMap::clear`.
    pub fn clear(&mut self) {
        self.direct_pages.clear();
        self.sparse_pages.clear();
        self.page_pool.clear();
    }

    /// Resets every live entry to vacant while retaining all allocated
    /// pages, and drops pooled pages. Lets dense maps clear without
    /// releasing (and reallocating) their page footprint.
    pub fn clear_values(&mut self) {
        let vacant = self.vacant;
        for page in self.direct_pages.iter_mut().flatten() {
            page.fill(vacant);
        }
        for page in self.sparse_pages.values_mut() {
            page.fill(vacant);
        }
        self.page_pool.clear();
    }

    /// Parks every live page for reuse by an imminent full rebuild, dropping
    /// all entries. The direct vector keeps its buffer, mirroring how a dense
    /// vector keeps its backing allocation, but the sparse directory is taken
    /// (not drained) so its buckets are released: a temporarily large sparse
    /// population must not leave bucket capacity proportional to its peak
    /// after the live set shrinks. Must be paired with
    /// [`PagedIndex::end_rebuild`].
    pub fn begin_rebuild(&mut self) {
        debug_assert!(
            self.page_pool.is_empty(),
            "paged index rebuilds must not nest"
        );
        self.page_pool
            .extend(self.direct_pages.iter_mut().filter_map(|slot| slot.take()));
        self.direct_pages.clear();
        let sparse = std::mem::take(&mut self.sparse_pages);
        self.page_pool.extend(sparse.into_values());
    }

    /// Drops pooled pages the rebuild did not reuse, so the pool never
    /// outlives the rebuild it served. Call once the rebuild has assigned
    /// every live entry.
    pub fn end_rebuild(&mut self) {
        self.page_pool = Vec::new();
    }

    /// Drops every occupied entry whose raw index is rejected by `keep`,
    /// freeing pages left empty, then compacts the sparse directory so its
    /// buckets track the live set rather than the pruned peak. Removal order
    /// does not affect the outcome, so hash-map iteration order cannot leak
    /// into caller behavior.
    pub fn retain(&mut self, mut keep: impl FnMut(u64) -> bool) {
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
        // Pruning runs on full rebuilds, so compact the directory here:
        // removals alone would leave buckets proportional to the pruned peak.
        // The resulting capacity depends only on the live count, keeping
        // rebuilds deterministic.
        self.sparse_pages.shrink_to_fit();
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
                .expect("direct paged-index page should exist after pooling");
        }
        if !self.sparse_pages.contains_key(&page_id) {
            let page = self.pooled_page();
            self.sparse_pages.insert(page_id, page);
        }
        self.sparse_pages
            .get_mut(&page_id)
            .map(Box::as_mut)
            .expect("sparse paged-index page should exist after pooling")
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
    pub fn occupied_entries(&self) -> Vec<(u64, T)> {
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
    pub fn allocated_pages(&self) -> usize {
        self.direct_pages
            .iter()
            .filter(|slot| slot.is_some())
            .count()
            + self.sparse_pages.len()
    }

    /// Counts allocated pages in the direct vector.
    pub fn direct_allocated_pages(&self) -> usize {
        self.direct_pages
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }

    /// Counts allocated pages in the sparse directory.
    pub fn sparse_allocated_pages(&self) -> usize {
        self.sparse_pages.len()
    }

    /// Counts pooled pages parked for the active rebuild.
    pub fn pooled_pages(&self) -> usize {
        self.page_pool.len()
    }

    /// Estimates all heap held for this index: live and pooled value pages
    /// plus the page-directory buffers (direct vector, sparse-map buckets,
    /// and pool vector). Sparse buckets use a conservative per-bucket
    /// estimate, so the total may slightly overstate rather than understate.
    pub fn storage_bytes(&self) -> usize {
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

impl<T> PartialEq for PagedIndex<T>
where
    T: Copy + PartialEq,
{
    /// Compares logical contents only: pooled boxes and page layout do not
    /// affect equality.
    fn eq(&self, other: &Self) -> bool {
        self.vacant == other.vacant && self.occupied_entries() == other.occupied_entries()
    }
}

impl<T> Eq for PagedIndex<T> where T: Copy + PartialEq + Eq {}

impl<T> Hash for PagedIndex<T>
where
    T: Copy + PartialEq + Hash,
{
    /// Hashes the logical contents in index order so equal indexes hash
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

impl<T> std::fmt::Debug for PagedIndex<T>
where
    T: Copy + PartialEq + std::fmt::Debug,
{
    /// Prints logical contents only; pooled boxes are omitted.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagedIndex")
            .field("vacant", &self.vacant)
            .field("entries", &self.occupied_entries())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_high_indices_allocate_only_their_pages() {
        let mut index = PagedIndex::new(u32::MAX);
        index.insert(3, 7);
        index.insert(10_000_000 * 4 + 1, 9);

        assert_eq!(index.get(3), Some(7));
        assert_eq!(index.get(10_000_000 * 4 + 1), Some(9));
        assert_eq!(index.get(4), None);
        assert_eq!(index.allocated_pages(), 2);
        assert_eq!(index.direct_allocated_pages(), 1);
        assert_eq!(index.sparse_allocated_pages(), 1);
    }

    #[test]
    fn removing_last_entry_frees_its_page() {
        let mut index = PagedIndex::new(u32::MAX);
        let high = u64::from(MAX_DIRECT_PAGES as u32) * PAGE_SIZE as u64 + 5;
        index.insert(0, 1);
        index.insert(high, 2);
        assert_eq!(index.allocated_pages(), 2);

        assert_eq!(index.remove(high), Some(2));
        assert_eq!(index.get(high), None);
        assert_eq!(index.allocated_pages(), 1);

        assert_eq!(index.remove(0), Some(1));
        assert_eq!(index.allocated_pages(), 0);
        assert_eq!(index, PagedIndex::new(u32::MAX));
    }

    #[test]
    fn contains_tracks_occupancy_without_storing_vacant() {
        let mut index = PagedIndex::new(0_u64);
        assert!(!index.contains(9));
        assert_eq!(index.vacant(), 0);
        index.insert(9, 4);
        assert!(index.contains(9));
        assert_eq!(index.remove(9), Some(4));
        assert!(!index.contains(9));
        assert_eq!(index.allocated_pages(), 0);
    }

    #[test]
    fn retain_drops_rejected_indices_and_frees_empty_pages() {
        let mut index = PagedIndex::new(0_u64);
        index.insert(7, 3);
        index.insert(261, 5);
        index.insert(1_100_000, 9);

        index.retain(|raw| raw != 261);

        assert_eq!(index.get(7), Some(3));
        assert_eq!(index.get(261), None);
        assert_eq!(index.get(1_100_000), Some(9));
        assert_eq!(index.allocated_pages(), 2);
    }

    #[test]
    fn rebuild_scope_reuses_pages_then_drops_remainder() {
        let mut index = PagedIndex::new(u32::MAX);
        index.insert(3, 7);
        index.insert(900_000, 9);
        assert_eq!(index.allocated_pages(), 2);

        index.begin_rebuild();
        assert_eq!(index.get(3), None);
        assert_eq!(index.allocated_pages(), 0);
        assert_eq!(index.pooled_pages(), 2);

        index.insert(300, 11);
        assert_eq!(index.get(300), Some(11));
        assert_eq!(index.pooled_pages(), 1);
        assert_eq!(index.allocated_pages(), 1);

        index.end_rebuild();
        assert_eq!(index.pooled_pages(), 0);
        assert_eq!(index.get(300), Some(11));
    }

    #[test]
    fn clear_values_retains_pages_while_clear_drops_them() {
        let mut index = PagedIndex::new(u32::MAX);
        index.insert(1, 10);
        index.insert(900_000, 20);
        assert_eq!(index.allocated_pages(), 2);

        index.clear_values();
        assert_eq!(index.get(1), None);
        assert_eq!(index.allocated_pages(), 2);

        index.insert(2, 30);
        assert_eq!(index.get(2), Some(30));

        index.clear();
        assert_eq!(index.allocated_pages(), 0);
        assert_eq!(index.get(2), None);
    }

    #[test]
    fn equal_contents_compare_equal_regardless_of_page_layout() {
        use std::collections::hash_map::DefaultHasher;

        let mut first = PagedIndex::new(u32::MAX);
        first.insert(1, 10);
        first.insert(900_000, 20);
        first.remove(1);
        first.insert(1, 10);

        let mut second = PagedIndex::new(u32::MAX);
        second.insert(900_000, 20);
        second.insert(1, 10);

        assert_eq!(first, second);
        let hash = |index: &PagedIndex<u32>| {
            let mut hasher = DefaultHasher::new();
            index.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(&first), hash(&second));
    }
}
