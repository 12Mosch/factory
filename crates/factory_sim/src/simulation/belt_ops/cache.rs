use super::types::{TRANSPORT_LANE_SLOTS_PER_ENTITY, TransportLaneKey, TransportRunIndex};
use super::*;

mod activity;
mod graph;
mod item_tracking;

pub(in crate::simulation::belt_ops) use item_tracking::mark_item_revision;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportDirtyRegion {
    pub(in crate::simulation) entity_id: EntityId,
    pub(in crate::simulation) footprint: EntityFootprint,
}

/// Subsystem-owned cache for belt/splitter transport.
///
/// This holds no authoritative simulation state: the durable belt/transport
/// data (lanes, item positions, splitter cursors) lives in [`EntityStore`].
/// The graph is a derived adjacency index rebuilt from `entities` whenever the
/// transport topology changes, `active_runs` is the advancement work queue,
/// and `visit_states` is reusable per-tick traversal scratch.
/// All of it is `#[serde(skip)]` and reconstructed on load.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::simulation) struct TransportLaneCache {
    dirty: bool,
    /// Scoped edits since the last refresh, applied as an incremental patch
    /// unless `dirty` forces a full rebuild.
    dirty_regions: Vec<TransportDirtyRegion>,
    /// Monotonic change tokens consumed by incremental presentation. These
    /// are derived runtime state; saves reconstruct presentation from scratch.
    pub(in crate::simulation) item_revision: u64,
    pub(in crate::simulation) item_revisions_by_entity: Vec<u64>,
    next_item_id: u64,
    pub(in crate::simulation) graph: TransportLaneGraph,
    pub(in crate::simulation) visit_states: TransportRunVisitStorage,
    pub(in crate::simulation) active_runs: TransportRunActiveStorage,
    #[cfg(test)]
    pub(in crate::simulation) rebuilds: u64,
    #[cfg(test)]
    pub(in crate::simulation) patches: u64,
}

impl Default for TransportLaneCache {
    fn default() -> Self {
        Self {
            dirty: true,
            dirty_regions: Vec::new(),
            item_revision: 0,
            item_revisions_by_entity: Vec::new(),
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

    fn rebuild_all(&mut self, entities: &EntityStore) {
        self.graph.rebuild(entities);
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
