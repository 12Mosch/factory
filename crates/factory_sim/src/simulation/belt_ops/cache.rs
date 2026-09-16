use super::types::{TRANSPORT_LANE_SLOTS_PER_ENTITY, TransportLaneKey, TransportRunIndex};
use super::*;

mod activity;
mod graph;
mod item_tracking;
mod paged;

pub(in crate::simulation::belt_ops) use item_tracking::mark_item_revision;
pub(in crate::simulation) use paged::EntityItemRevisionMap;

pub(in crate::simulation) use activity::{TransportRunActiveStorage, TransportRunVisitStorage};
pub(in crate::simulation) use graph::TransportLaneGraph;

/// Most scoped edits carried between refreshes before the cache falls back to
/// a full rebuild.
const MAX_DIRTY_REGIONS: usize = 32;
pub(super) const PATCH_STORAGE_HEADROOM: usize =
    MAX_DIRTY_REGIONS * TRANSPORT_LANE_SLOTS_PER_ENTITY;

/// One transport-affecting entity edit since the last refresh. The patch
/// re-resolves lane geometry for entities whose downstream resolution can see
/// these tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub(in crate::simulation) struct TransportDirtyRegion {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) footprint: EntityFootprint,
}

/// Belt transport execution state. The active-run order and upstream wake
/// boundaries affect subsequent ticks. Preserve their graph, including the
/// incremental slot/run layout and pending edits, so future patches and cyclic
/// traversal use the same ordering after load. Only visit scratch and
/// presentation change tokens are reconstructed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub(in crate::simulation) struct TransportLaneCache {
    dirty: bool,
    /// Scoped edits since the last refresh, applied as an incremental patch
    /// unless `dirty` forces a full rebuild.
    dirty_regions: Vec<TransportDirtyRegion>,
    /// Monotonic change tokens consumed by incremental presentation. These
    /// are derived runtime state; saves reconstruct presentation from scratch.
    #[serde(skip)]
    pub(in crate::simulation) item_revision: u64,
    #[serde(skip)]
    pub(in crate::simulation) item_revisions_by_entity: EntityItemRevisionMap,
    pub(in crate::simulation) next_item_id: u64,
    pub(in crate::simulation) graph: TransportLaneGraph,
    #[serde(skip)]
    pub(in crate::simulation) visit_states: TransportRunVisitStorage,
    pub(in crate::simulation) active_runs: TransportRunActiveStorage,
    #[cfg(test)]
    #[serde(skip)]
    pub(in crate::simulation) rebuilds: u64,
    #[cfg(test)]
    #[serde(skip)]
    pub(in crate::simulation) patches: u64,
}

impl Default for TransportLaneCache {
    fn default() -> Self {
        Self {
            dirty: true,
            dirty_regions: Vec::new(),
            item_revision: 0,
            item_revisions_by_entity: EntityItemRevisionMap::default(),
            next_item_id: 1,
            graph: TransportLaneGraph::default(),
            visit_states: TransportRunVisitStorage::default(),
            active_runs: TransportRunActiveStorage::default(),
            #[cfg(test)]
            rebuilds: 0,
            #[cfg(test)]
            patches: 0,
        }
    }
}

impl TransportLaneCache {
    /// Drops the queue while retaining its generation marks to model corrupt input.
    #[cfg(test)]
    pub(in crate::simulation) fn corrupt_active_queue_for_test(&mut self) {
        self.active_runs.runs.clear();
    }

    /// Copies durable transport work while resetting runtime scratch and counters.
    pub(in crate::simulation) fn clone_for_save(&self) -> Self {
        Self {
            dirty: self.dirty,
            dirty_regions: self.dirty_regions.clone(),
            next_item_id: self.next_item_id,
            graph: self.graph.clone(),
            active_runs: self.active_runs.clone(),
            visit_states: TransportRunVisitStorage::default(),
            item_revision: 0,
            item_revisions_by_entity: EntityItemRevisionMap::default(),
            #[cfg(test)]
            rebuilds: 0,
            #[cfg(test)]
            patches: 0,
        }
    }

    /// Hashes only transport state that can affect subsequent simulation ticks.
    pub(in crate::simulation) fn hash_durable<H: Hasher>(&self, state: &mut H) {
        self.dirty.hash(state);
        self.dirty_regions.hash(state);
        self.next_item_id.hash(state);
        self.graph.hash(state);
        self.active_runs.hash(state);
    }

    /// Validates saved patch work, execution graph, and active scheduling state.
    pub(in crate::simulation) fn validate_work(
        &self,
        entities: &EntityStore,
        world: &WorldSim,
    ) -> Result<(), SimValidationError> {
        if self.dirty_regions.len() > MAX_DIRTY_REGIONS
            || self.dirty_regions.iter().any(|region| {
                !validation::world::valid_work_footprint(world, region.footprint)
                    || entities
                        .placed_entities
                        .get(&region.entity_id)
                        .is_some_and(|_| {
                            !entities.transport_belts.contains_key(&region.entity_id)
                                && !entities.splitters.contains_key(&region.entity_id)
                        })
            })
            || !self
                .graph
                .is_valid(entities, !self.dirty && self.dirty_regions.is_empty())
            || !self.active_runs.is_valid(&self.graph)
        {
            return Err(SimValidationError::InvalidTransportWork);
        }
        Ok(())
    }

    pub(in crate::simulation) fn invalidate(&mut self) {
        self.dirty = true;
        self.dirty_regions.clear();
    }

    pub(in crate::simulation) fn invalidate_region(&mut self, region: TransportDirtyRegion) {
        if self.dirty {
            return;
        }
        if self.dirty_regions.len() >= MAX_DIRTY_REGIONS {
            self.invalidate();
            return;
        }
        self.dirty_regions.push(region);
    }

    /// Applies pending topology edits as an incremental patch, falling back
    /// to a full rebuild when the patch cannot cover them. Prunes revision
    /// tokens for entities that no longer exist.
    pub(in crate::simulation) fn refresh(
        &mut self,
        entities: &EntityStore,
        catalog_underground_distance: impl FnOnce() -> u8,
    ) {
        if self.dirty {
            self.rebuild_all(entities);
            return;
        }
        if self.dirty_regions.is_empty() {
            return;
        }

        let catalog_underground_distance = catalog_underground_distance();
        let regions = std::mem::take(&mut self.dirty_regions);
        match self
            .graph
            .patch(entities, &regions, catalog_underground_distance)
        {
            Some(new_runs) => {
                // Mirror slot freeing: tokens for entities that no longer
                // exist must go, or revision storage would track the ID
                // high-water mark instead of live transport state.
                for region in &regions {
                    if !entities.transport_belts.contains_key(&region.entity_id)
                        && !entities.splitters.contains_key(&region.entity_id)
                    {
                        self.item_revisions_by_entity.remove(region.entity_id);
                    }
                }
                for run in new_runs {
                    self.activate_run_from_items(entities, TransportRunIndex::from_index(run));
                }
                #[cfg(test)]
                {
                    self.patches += 1;
                }
            }
            None => self.rebuild_all(entities),
        }
    }

    /// Rebuilds the lane graph and active runs from scratch, pruning dead
    /// revision tokens so they track live transport state.
    fn rebuild_all(&mut self, entities: &EntityStore) {
        self.graph.rebuild(entities);
        // Full rebuilds bypass incremental slot freeing, so prune dead
        // revision tokens here to keep them proportional to live transport.
        self.item_revisions_by_entity
            .retain_live_transport(entities);
        self.active_runs
            .rebuild_from_entities(entities, &self.graph);
        self.dirty = false;
        self.dirty_regions.clear();
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
    }

    /// Wakes a run created by an incremental patch at its most upstream lane
    /// that holds items, mirroring what a full active-set rebuild derives.
    fn activate_run_from_items(&mut self, entities: &EntityStore, run: TransportRunIndex) {
        let position =
            self.graph
                .run_lanes(run)
                .iter()
                .enumerate()
                .find_map(|(position, &slot)| {
                    let key = self.graph.key_for(slot)?;
                    let has_items = match key {
                        TransportLaneKey::Belt {
                            entity_id,
                            lane_index,
                        } => entities
                            .transport_belts
                            .get(&entity_id)
                            .and_then(|segment| segment.lanes.get(lane_index))
                            .is_some_and(|lane| !lane.items.is_empty()),
                        TransportLaneKey::Splitter {
                            entity_id,
                            input_port,
                            lane_index,
                        } => entities
                            .splitters
                            .get(&entity_id)
                            .and_then(|state| state.input_lanes.get(input_port))
                            .and_then(|lanes| lanes.get(lane_index))
                            .is_some_and(|lane| !lane.items.is_empty()),
                    };
                    has_items.then_some(position)
                });
        if let Some(position) = position {
            self.active_runs.mark_active(run, position);
        }
    }

    pub(in crate::simulation) fn mark_active(&mut self, key: TransportLaneKey) {
        if let Some(index) = self.graph.slot_for(key)
            && let Some((run, position)) = self.graph.run_and_position_for_slot(index)
        {
            self.active_runs.mark_active(run, position);
        }
    }

    /// Estimates all heap held for the per-entity revision index, including
    /// retained pages and directory buffers.
    #[cfg(test)]
    pub(in crate::simulation) fn item_revision_storage_bytes(&self) -> usize {
        self.item_revisions_by_entity.storage_bytes()
    }

    pub(in crate::simulation) fn mark_active_with_upstreams(&mut self, key: TransportLaneKey) {
        let Some(index) = self.graph.slot_for(key) else {
            return;
        };
        if let Some((run, position)) = self.graph.run_and_position_for_slot(index) {
            self.active_runs.mark_active(run, position);
        }
        for &upstream in self.graph.upstream_for(index) {
            if let Some((run, position)) = self.graph.run_and_position_for_slot(upstream) {
                self.active_runs.mark_active(run, position);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn catalog_distance_is_resolved_only_for_incremental_patches() {
        let entities = EntityStore::empty();
        let mut cache = TransportLaneCache::default();
        let resolutions = Cell::new(0);
        let resolve_distance = || {
            resolutions.set(resolutions.get() + 1);
            8
        };

        cache.refresh(&entities, resolve_distance);
        cache.refresh(&entities, resolve_distance);
        assert_eq!(resolutions.get(), 0);

        cache.invalidate_region(TransportDirtyRegion {
            entity_id: EntityId::new(1),
            footprint: EntityFootprint::single_tile(0, 0),
        });
        cache.refresh(&entities, resolve_distance);
        assert_eq!(resolutions.get(), 1);
    }
}
