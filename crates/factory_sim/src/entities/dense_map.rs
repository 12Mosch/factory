use crate::ids::EntityId;
use crate::paged_index::PagedIndex;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

const VACANT_INDEX: u32 = u32::MAX;

/// Compact entity state storage with constant-time lookup by [`EntityId`].
///
/// Entity ids index sparse, fixed-size indirection pages while occupied values
/// stay contiguous. Removal uses `swap_remove`, so deleting an entity does not
/// leave holes in the hot state array. Iteration and serialization still use
/// entity-id order to preserve deterministic behavior and the existing save
/// representation.
///
/// The indirection directory is the shared [`PagedIndex`] primitive (256-entry
/// pages, a direct page vector with a ~4096-page cutoff, sparse hash-backed
/// pages, allocate-on-write / free-when-empty), holding `u32` entry positions
/// with `u32::MAX` as the vacant sentinel.
#[derive(Clone, Debug)]
pub(crate) struct DenseEntityMap<T> {
    index: PagedIndex<u32>,
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
        self.index.insert(id.raw(), entry_index);
        let inserted = self.ordered_ids.insert(id);
        debug_assert!(inserted);
        None
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<T> {
        let entry_index = self.index.remove(id.raw())?;
        let removed_id = self.ordered_ids.remove(id);
        debug_assert!(removed_id);

        let removed = self.entries.swap_remove(entry_index as usize);
        debug_assert_eq!(removed.id, *id);
        if (entry_index as usize) < self.entries.len() {
            let moved_id = self.entries[entry_index as usize].id;
            self.index.insert(moved_id.raw(), entry_index);
        }
        Some(removed.value)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.index.clear_values();
        self.ordered_ids.clear();
        self.entries.clear();
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
        self.index
            .get(id.raw())
            .map(|entry_index| entry_index as usize)
    }
}

impl<T> Default for DenseEntityMap<T> {
    fn default() -> Self {
        Self {
            index: PagedIndex::new(VACANT_INDEX),
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
        assert_eq!(map.index.direct_allocated_pages(), 1);
        assert_eq!(map.index.sparse_allocated_pages(), 1);
        assert_eq!(
            map.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [low, high]
        );
    }

    #[test]
    fn removing_last_entry_of_a_page_frees_it() {
        let mut map = DenseEntityMap::default();
        let low = EntityId::new(1);
        // Second entry of the same low page: shares the page with `low`.
        let low_sibling = EntityId::new(2);
        let high = EntityId::new(u64::MAX);

        map.insert(low, 10);
        map.insert(low_sibling, 11);
        map.insert(high, 20);
        assert_eq!(map.index.allocated_pages(), 2);

        assert_eq!(map.remove(&high), Some(20));
        assert_eq!(map.get(&high), None);
        assert_eq!(map.index.allocated_pages(), 1);

        // Swap-remove moves the last entry; the survivors must stay addressable.
        assert_eq!(map.get(&low), Some(&10));
        assert_eq!(map.get(&low_sibling), Some(&11));
    }

    #[test]
    fn clear_retains_indirection_pages_for_reuse() {
        let mut map = DenseEntityMap::default();
        map.insert(EntityId::new(1), 10);
        map.insert(EntityId::new(u64::MAX), 20);
        assert_eq!(map.index.allocated_pages(), 2);

        map.clear();
        assert!(map.is_empty());
        assert_eq!(map.index.allocated_pages(), 2);
        assert_eq!(map.get(&EntityId::new(1)), None);

        map.insert(EntityId::new(3), 30);
        assert_eq!(map.get(&EntityId::new(3)), Some(&30));
        assert_eq!(map.index.allocated_pages(), 2);
    }
}
