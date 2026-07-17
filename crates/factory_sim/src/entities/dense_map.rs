use crate::ids::EntityId;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

const VACANT_INDEX: u32 = u32::MAX;

/// Compact entity state storage with constant-time lookup by [`EntityId`].
///
/// Entity ids index a small indirection table while occupied values stay
/// contiguous. Removal uses `swap_remove`, so deleting an entity does not
/// leave holes in the hot state array. Iteration and serialization still use
/// entity-id order to preserve deterministic behavior and the existing save
/// representation.
#[derive(Clone, Debug)]
pub(crate) struct DenseEntityMap<T> {
    indices: Vec<u32>,
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
        let id_index =
            usize::try_from(id.raw()).expect("entity ids must fit the platform address space");
        if self.indices.len() <= id_index {
            self.indices.resize(id_index + 1, VACANT_INDEX);
        }

        let occupied_index = self.indices[id_index];
        if occupied_index != VACANT_INDEX {
            let entry = &mut self.entries[occupied_index as usize];
            debug_assert_eq!(entry.id, id);
            return Some(std::mem::replace(&mut entry.value, value));
        }

        let entry_index =
            u32::try_from(self.entries.len()).expect("dense entity state capacity exceeded");
        self.entries.push(DenseEntityEntry { id, value });
        self.indices[id_index] = entry_index;
        None
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<T> {
        let id_index = usize::try_from(id.raw()).ok()?;
        let slot = self.indices.get_mut(id_index)?;
        let entry_index = std::mem::replace(slot, VACANT_INDEX);
        if entry_index == VACANT_INDEX {
            return None;
        }

        let removed = self.entries.swap_remove(entry_index as usize);
        debug_assert_eq!(removed.id, *id);
        if (entry_index as usize) < self.entries.len() {
            let moved_id = self.entries[entry_index as usize].id;
            let moved_id_index = usize::try_from(moved_id.raw())
                .expect("stored entity ids must fit the platform address space");
            self.indices[moved_id_index] = entry_index;
        }
        Some(removed.value)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn iter(&self) -> DenseEntityIter<'_, T> {
        DenseEntityIter {
            next_id_index: 0,
            indices: &self.indices,
            entries: &self.entries,
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
        let id_index = usize::try_from(id.raw()).ok()?;
        let &entry_index = self.indices.get(id_index)?;
        (entry_index != VACANT_INDEX).then_some(entry_index as usize)
    }
}

impl<T> Default for DenseEntityMap<T> {
    fn default() -> Self {
        Self {
            indices: Vec::new(),
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
    next_id_index: usize,
    indices: &'a [u32],
    entries: &'a [DenseEntityEntry<T>],
}

impl<'a, T> Iterator for DenseEntityIter<'a, T> {
    type Item = (&'a EntityId, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(&entry_index) = self.indices.get(self.next_id_index) {
            self.next_id_index += 1;
            if entry_index == VACANT_INDEX {
                continue;
            }
            let entry = &self.entries[entry_index as usize];
            return Some((&entry.id, &entry.value));
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.entries.len()))
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
}
