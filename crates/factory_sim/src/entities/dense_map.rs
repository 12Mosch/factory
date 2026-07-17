use crate::ids::EntityId;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

const VACANT_INDEX: u32 = u32::MAX;
const INDIRECTION_PAGE_BITS: u32 = 8;
const INDIRECTION_PAGE_SIZE: usize = 1 << INDIRECTION_PAGE_BITS;
const MAX_DIRECT_INDIRECTION_PAGES: usize = 4_096;
type IndirectionPage = [u32; INDIRECTION_PAGE_SIZE];

/// Compact entity state storage with constant-time lookup by [`EntityId`].
///
/// Entity ids index sparse, fixed-size indirection pages while occupied values
/// stay contiguous. Removal uses `swap_remove`, so deleting an entity does not
/// leave holes in the hot state array. Iteration and serialization still use
/// entity-id order to preserve deterministic behavior and the existing save
/// representation.
#[derive(Clone, Debug)]
pub(crate) struct DenseEntityMap<T> {
    direct_pages: Vec<Option<Box<IndirectionPage>>>,
    sparse_pages: HashMap<u64, Box<IndirectionPage>>,
    ordered_ids: BTreeSet<EntityId>,
    entries: Vec<DenseEntityEntry<T>>,
}

#[derive(Clone, Debug)]
struct DenseEntityEntry<T> {
    id: EntityId,
    value: T,
}

impl<T> DenseEntityMap<T> {
    pub(crate) fn get(&self, id: &EntityId) -> Option<&T> {
        let entry = self.entry(*id)?;
        Some(&entry.value)
    }

    pub(crate) fn get_mut(&mut self, id: &EntityId) -> Option<&mut T> {
        let entry_index = self.entry_index(*id)?;
        let entry = self.entries.get_mut(entry_index)?;
        debug_assert_eq!(entry.id, *id);
        Some(&mut entry.value)
    }

    pub(crate) fn contains_key(&self, id: &EntityId) -> bool {
        self.entry_index(*id).is_some()
    }

    pub(crate) fn insert(&mut self, id: EntityId, value: T) -> Option<T> {
        if let Some(occupied_index) = self.entry_index(id) {
            let entry = &mut self.entries[occupied_index];
            debug_assert_eq!(entry.id, id);
            return Some(std::mem::replace(&mut entry.value, value));
        }

        let entry_index =
            u32::try_from(self.entries.len()).expect("dense entity state capacity exceeded");
        self.entries.push(DenseEntityEntry { id, value });
        let (page_id, page_offset) = indirection_location(id);
        let page = self.page_mut_or_insert(page_id);
        debug_assert_eq!(page[page_offset], VACANT_INDEX);
        page[page_offset] = entry_index;
        let inserted = self.ordered_ids.insert(id);
        debug_assert!(inserted);
        None
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<T> {
        let (page_id, page_offset) = indirection_location(*id);
        let page = self.page_mut(page_id)?;
        let entry_index = std::mem::replace(&mut page[page_offset], VACANT_INDEX);
        if entry_index == VACANT_INDEX {
            return None;
        }
        let page_is_empty = page.iter().all(|index| *index == VACANT_INDEX);
        if page_is_empty {
            self.remove_page(page_id);
        }
        let removed_id = self.ordered_ids.remove(id);
        debug_assert!(removed_id);

        let removed = self.entries.swap_remove(entry_index as usize);
        debug_assert_eq!(removed.id, *id);
        if (entry_index as usize) < self.entries.len() {
            let moved_id = self.entries[entry_index as usize].id;
            let (moved_page_id, moved_page_offset) = indirection_location(moved_id);
            self.page_mut(moved_page_id)
                .expect("stored entity indirection page should exist")[moved_page_offset] =
                entry_index;
        }
        Some(removed.value)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn iter(&self) -> DenseEntityIter<'_, T> {
        DenseEntityIter {
            ids: self.ordered_ids.iter(),
            map: self,
        }
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &EntityId> {
        self.iter().map(|(id, _)| id)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, value)| value)
    }

    fn entry(&self, id: EntityId) -> Option<&DenseEntityEntry<T>> {
        let entry_index = self.entry_index(id)?;
        let entry = self.entries.get(entry_index)?;
        debug_assert_eq!(entry.id, id);
        Some(entry)
    }

    fn entry_index(&self, id: EntityId) -> Option<usize> {
        let (page_id, page_offset) = indirection_location(id);
        let &entry_index = self.page(page_id)?.get(page_offset)?;
        (entry_index != VACANT_INDEX).then_some(entry_index as usize)
    }

    fn page(&self, page_id: u64) -> Option<&IndirectionPage> {
        if page_id < MAX_DIRECT_INDIRECTION_PAGES as u64 {
            return self
                .direct_pages
                .get(page_id as usize)?
                .as_ref()
                .map(Box::as_ref);
        }
        self.sparse_pages.get(&page_id).map(Box::as_ref)
    }

    fn page_mut(&mut self, page_id: u64) -> Option<&mut IndirectionPage> {
        if page_id < MAX_DIRECT_INDIRECTION_PAGES as u64 {
            return self
                .direct_pages
                .get_mut(page_id as usize)?
                .as_mut()
                .map(Box::as_mut);
        }
        self.sparse_pages.get_mut(&page_id).map(Box::as_mut)
    }

    fn page_mut_or_insert(&mut self, page_id: u64) -> &mut IndirectionPage {
        if page_id < MAX_DIRECT_INDIRECTION_PAGES as u64 {
            let page_index = page_id as usize;
            if self.direct_pages.len() <= page_index {
                self.direct_pages.resize_with(page_index + 1, || None);
            }
            return self.direct_pages[page_index]
                .get_or_insert_with(|| Box::new([VACANT_INDEX; INDIRECTION_PAGE_SIZE]));
        }
        self.sparse_pages
            .entry(page_id)
            .or_insert_with(|| Box::new([VACANT_INDEX; INDIRECTION_PAGE_SIZE]))
    }

    fn remove_page(&mut self, page_id: u64) {
        if page_id < MAX_DIRECT_INDIRECTION_PAGES as u64 {
            self.direct_pages[page_id as usize] = None;
        } else {
            self.sparse_pages.remove(&page_id);
        }
    }
}

fn indirection_location(id: EntityId) -> (u64, usize) {
    let raw = id.raw();
    (
        raw >> INDIRECTION_PAGE_BITS,
        (raw & (INDIRECTION_PAGE_SIZE as u64 - 1)) as usize,
    )
}

impl<T> Default for DenseEntityMap<T> {
    fn default() -> Self {
        Self {
            direct_pages: Vec::new(),
            sparse_pages: HashMap::new(),
            ordered_ids: BTreeSet::new(),
            entries: Vec::new(),
        }
    }
}

impl<T: PartialEq> PartialEq for DenseEntityMap<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(id, value)| other.get(id) == Some(value))
    }
}

impl<T: Eq> Eq for DenseEntityMap<T> {}

impl<T: Hash> Hash for DenseEntityMap<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for entry in self.iter() {
            entry.hash(state);
        }
    }
}

impl<T: Serialize> Serialize for DenseEntityMap<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (id, value) in self {
            map.serialize_entry(id, value)?;
        }
        map.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for DenseEntityMap<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entries = BTreeMap::<EntityId, T>::deserialize(deserializer)?;
        let mut dense = Self::default();
        for (id, value) in entries {
            dense.insert(id, value);
        }
        Ok(dense)
    }
}

impl<'a, T> IntoIterator for &'a DenseEntityMap<T> {
    type Item = (&'a EntityId, &'a T);
    type IntoIter = DenseEntityIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub(crate) struct DenseEntityIter<'a, T> {
    ids: std::collections::btree_set::Iter<'a, EntityId>,
    map: &'a DenseEntityMap<T>,
}

impl<'a, T> Iterator for DenseEntityIter<'a, T> {
    type Item = (&'a EntityId, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.next()?;
        let entry_index = self
            .map
            .entry_index(*id)
            .expect("iterated entity indirection should exist");
        let entry = &self.map.entries[entry_index];
        debug_assert_eq!(&entry.id, id);
        Some((&entry.id, &entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ids.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_and_removal_keep_values_dense() {
        let mut map = DenseEntityMap::default();
        let first = EntityId::new(2);
        let middle = EntityId::new(5);
        let last = EntityId::new(9);
        map.insert(first, 20);
        map.insert(middle, 50);
        map.insert(last, 90);

        assert_eq!(map.remove(&middle), Some(50));
        assert_eq!(map.get(&first), Some(&20));
        assert_eq!(map.get(&middle), None);
        assert_eq!(map.get(&last), Some(&90));
        assert_eq!(map.entries.len(), 2);
    }

    #[test]
    fn serialization_matches_btree_map_representation() {
        let mut dense = DenseEntityMap::default();
        dense.insert(EntityId::new(9), 90_u16);
        dense.insert(EntityId::new(2), 20);

        let expected = BTreeMap::from([(EntityId::new(2), 20_u16), (EntityId::new(9), 90)]);

        assert_eq!(
            bincode::serialize(&dense).expect("dense map should serialize"),
            bincode::serialize(&expected).expect("tree map should serialize")
        );
        let restored: DenseEntityMap<u16> =
            bincode::deserialize(&bincode::serialize(&dense).unwrap()).unwrap();
        assert_eq!(restored, dense);
    }

    #[test]
    fn sparse_high_ids_allocate_only_their_indirection_pages() {
        let mut map = DenseEntityMap::default();
        let low = EntityId::new(1);
        let high = EntityId::new(u64::MAX);

        map.insert(low, 10);
        map.insert(high, 20);

        assert_eq!(map.get(&low), Some(&10));
        assert_eq!(map.get(&high), Some(&20));
        assert_eq!(map.direct_pages.iter().flatten().count(), 1);
        assert_eq!(map.sparse_pages.len(), 1);
        assert_eq!(
            map.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [low, high]
        );
    }
}
