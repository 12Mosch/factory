//! Sparse paged index storage for belt-transport runtime caches.
//!
//! Entity IDs are allocated monotonically and never reused, so a vector
//! indexed directly by raw entity ID grows with the highest ID ever issued,
//! even after those entities are destroyed. The maps here store fixed-size
//! pages of 256 entries addressed by `index >> 8` via the shared
//! [`PagedIndex`](crate::paged_index::PagedIndex) primitive: low pages live in
//! a direct vector for single-indirection hot-path lookup, while high pages
//! live in a hash map so sparse high IDs allocate only their own pages. A page
//! freed by incremental entity removal is dropped, so repeated build/destroy
//! churn settles at a stable footprint instead of tracking the ID high-water
//! mark. Full rebuilds park live pages in a pool that is dropped when the
//! rebuild completes, so a shrinking factory releases its peak footprint.
//!
//! The pages hold plain copy values directly instead of dense entry records
//! because transport lookups only need one integer per index; the
//! entity-state and belt render caches reuse the same primitive for their
//! `u32` entry-index directories.

use crate::paged_index::PagedIndex;
use crate::simulation::{EntityId, EntityStore};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

/// Sentinel stored for unmapped lane slots. Graph call sites never observe
/// it: [`TransportLaneSlotMap`] filters vacant entries at its boundary and
/// reports vacancy as `None`.
pub(in crate::simulation::belt_ops) const VACANT_SLOT: u32 = u32::MAX;

/// Maps `entity_id * 4 + lane_offset` wakeup keys onto compact lane slots.
///
/// Pages are allocated for live transport entities only; removing an
/// entity's last lane frees its page when nothing else shares it, and full
/// rebuilds reuse parked pages, dropping the unneeded remainder when done.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneSlotMap {
    inner: PagedIndex<u32>,
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

    /// Returns logical mappings in ascending raw-index order for validation
    /// and deterministic serialization.
    pub(in crate::simulation::belt_ops::cache) fn occupied_entries(&self) -> Vec<(u64, u32)> {
        self.inner.occupied_entries()
    }

    /// Estimates all heap held for the slot index; see
    /// [`PagedIndex::storage_bytes`].
    #[cfg(test)]
    pub(in crate::simulation) fn storage_bytes(&self) -> usize {
        self.inner.storage_bytes()
    }
}

impl Serialize for TransportLaneSlotMap {
    /// Serializes logical mappings only; page layout and rebuild scratch do
    /// not affect durable transport execution state.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.occupied_entries().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TransportLaneSlotMap {
    /// Reconstructs sparse pages from the canonical logical mapping and
    /// rejects duplicate, vacant, or platform-unrepresentable keys.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries = Vec::<(u64, u32)>::deserialize(deserializer)?;
        let mut map = Self::default();
        for (raw, slot) in entries {
            let raw_index = usize::try_from(raw)
                .map_err(|_| D::Error::custom("transport lane key does not fit usize"))?;
            if slot == VACANT_SLOT || map.get(raw_index).is_some() {
                return Err(D::Error::custom(
                    "duplicate or vacant transport lane mapping",
                ));
            }
            map.insert(raw_index, slot);
        }
        Ok(map)
    }
}

impl Default for TransportLaneSlotMap {
    /// Defaults to the lane-slot vacant sentinel.
    fn default() -> Self {
        Self {
            inner: PagedIndex::new(VACANT_SLOT),
        }
    }
}

/// Per-entity belt-item change tokens consumed by incremental presentation.
///
/// Only transport entities that changed items carry a token; tokens for
/// destroyed entities are pruned on the next topology refresh so storage
/// scales with live transport state instead of the ID high-water mark.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct EntityItemRevisionMap {
    inner: PagedIndex<u64>,
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
    /// [`PagedIndex::storage_bytes`].
    #[cfg(test)]
    pub(in crate::simulation) fn storage_bytes(&self) -> usize {
        self.inner.storage_bytes()
    }
}

impl Default for EntityItemRevisionMap {
    fn default() -> Self {
        Self {
            inner: PagedIndex::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Page mechanics (sparse allocation, freeing empty pages, rebuild
    /// pooling, retain, layout-independent equality) are covered once by the
    /// shared primitive's tests; these cover wrapper-specific semantics:
    /// `usize` adaptation, the lane-slot sentinel, and the serde contract
    /// that transport persistence relies on.

    #[test]
    fn slot_map_adapts_usize_keys_with_lane_sentinel_default() {
        let mut map = TransportLaneSlotMap::default();
        let sparse = 10_000_000 * 4 + 1;
        map.insert(3, 7);
        map.insert(sparse, 9);

        assert_eq!(map.get(3), Some(7));
        assert_eq!(map.get(sparse), Some(9));
        assert_eq!(map.get(4), None);
        assert_eq!(map.remove(3), Some(7));
        assert_eq!(map.get(3), None);
        assert_eq!(map, {
            let mut expected = TransportLaneSlotMap::default();
            expected.insert(sparse, 9);
            expected
        });
    }

    #[test]
    fn slot_map_serde_round_trip_preserves_logical_mappings() {
        let mut map = TransportLaneSlotMap::default();
        // Insert out of order across direct and sparse pages; the serialized
        // form must be the canonical ascending mapping.
        map.insert(10_000_000 * 4 + 1, 9);
        map.insert(3, 7);

        assert_eq!(map.occupied_entries(), [(3, 7), (10_000_000 * 4 + 1, 9)]);

        let bytes = bincode::serialize(&map).expect("slot map should serialize");
        let restored: TransportLaneSlotMap =
            bincode::deserialize(&bytes).expect("slot map should deserialize");
        assert_eq!(restored, map);
        assert_eq!(restored.get(3), Some(7));
        assert_eq!(restored.get(10_000_000 * 4 + 1), Some(9));
    }

    #[test]
    fn slot_map_deserialize_rejects_duplicate_or_vacant_mappings() {
        for entries in [vec![(3_u64, 7_u32), (3, 8)], vec![(3_u64, VACANT_SLOT)]] {
            let bytes = bincode::serialize(&entries).expect("entries should serialize");
            assert!(
                bincode::deserialize::<TransportLaneSlotMap>(&bytes).is_err(),
                "entries {entries:?} should be rejected"
            );
        }
    }

    #[test]
    fn revision_map_tracks_entity_tokens_with_zero_default() {
        let mut map = EntityItemRevisionMap::default();
        let low = EntityId::new(7);
        let sparse = EntityId::new(1_100_000);

        assert_eq!(map.revision(low), 0);
        map.set(low, 3);
        map.set(sparse, 9);
        assert_eq!(map.revision(low), 3);
        assert_eq!(map.revision(sparse), 9);

        map.remove(low);
        assert_eq!(map.revision(low), 0);
        assert_eq!(map.revision(sparse), 9);
    }
}
