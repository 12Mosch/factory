use super::cache::mark_item_revision;
use super::cache::{TransportLaneActiveStorage, TransportLaneGraph, TransportLaneVisitStorage};
use super::lane_access::{
    belt_lane_can_accept_position, lane_mut, set_lane_items, take_lane_for_advancement,
};
use super::types::{
    BeltLaneVisitState, TransportLaneDownstream, TransportLaneIndex, TransportLaneKey,
    TransportLaneTraversalStep,
};
use super::*;

pub(in crate::simulation) struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    graph: &'a TransportLaneGraph,
    visit_states: &'a mut TransportLaneVisitStorage,
    active_lanes: &'a mut TransportLaneActiveStorage,
    item_revision: &'a mut u64,
    item_revisions_by_entity: &'a mut Vec<u64>,
}

impl<'a> TransportBeltAdvancement<'a> {
    pub(in crate::simulation) fn new(
        entities: &'a mut EntityStore,
        graph: &'a TransportLaneGraph,
        visit_states: &'a mut TransportLaneVisitStorage,
        active_lanes: &'a mut TransportLaneActiveStorage,
        item_revision: &'a mut u64,
        item_revisions_by_entity: &'a mut Vec<u64>,
    ) -> Self {
        Self {
            entities,
            graph,
            visit_states,
            active_lanes,
            item_revision,
            item_revisions_by_entity,
        }
    }

    pub(in crate::simulation) fn process_active_lanes(&mut self) {
        let mut traversal_stack = std::mem::take(&mut self.visit_states.traversal_stack);
        for index in 0..self.active_lanes.lanes.len() {
            self.process_lane(self.active_lanes.lanes[index], &mut traversal_stack);
        }
        self.visit_states.traversal_stack = traversal_stack;
    }

    fn process_lane(
        &mut self,
        root: TransportLaneIndex,
        traversal_stack: &mut Vec<TransportLaneTraversalStep>,
    ) {
        debug_assert!(traversal_stack.is_empty());
        traversal_stack.push(TransportLaneTraversalStep::Enter(root));

        while let Some(step) = traversal_stack.pop() {
            match step {
                TransportLaneTraversalStep::Enter(index) => {
                    match self.visit_state(index) {
                        Some(BeltLaneVisitState::Done | BeltLaneVisitState::Processing) => continue,
                        None => {}
                    }
                    let Some(key) = self.graph.key_for(index) else {
                        continue;
                    };

                    self.set_visit_state(index, BeltLaneVisitState::Processing);
                    traversal_stack.push(TransportLaneTraversalStep::Exit(index));
                    let downstream = self.downstream_lane_indices(index, key);
                    for downstream_index in downstream.into_iter().rev() {
                        if self.visit_state(downstream_index)
                            != Some(BeltLaneVisitState::Processing)
                        {
                            traversal_stack
                                .push(TransportLaneTraversalStep::Enter(downstream_index));
                        }
                    }
                }
                TransportLaneTraversalStep::Exit(index) => {
                    let Some(key) = self.graph.key_for(index) else {
                        continue;
                    };
                    if self.advance_lane_items(index, key) {
                        self.mark_upstream_lanes_active(index);
                    }
                    self.set_visit_state(index, BeltLaneVisitState::Done);
                }
            }
        }
    }

    fn downstream_lane_indices(
        &self,
        index: TransportLaneIndex,
        key: TransportLaneKey,
    ) -> SmallVec<[TransportLaneIndex; 2]> {
        match self.graph.downstream_for(index) {
            TransportLaneDownstream::Missing => SmallVec::new(),
            TransportLaneDownstream::Belt { downstream } => {
                let mut downstream_indices = SmallVec::new();
                if let Some(index) = downstream {
                    downstream_indices.push(index);
                }
                downstream_indices
            }
            TransportLaneDownstream::Splitter { outputs } => {
                let preferred = self.splitter_preferred_output_port(key);
                let mut downstream = SmallVec::new();
                for output_port in [preferred, 1 - preferred] {
                    if let Some(key) = outputs[output_port] {
                        downstream.push(key);
                    }
                }
                downstream
            }
        }
    }

    fn splitter_preferred_output_port(&self, key: TransportLaneKey) -> usize {
        let TransportLaneKey::Splitter {
            entity_id,
            lane_index,
            ..
        } = key
        else {
            return 0;
        };
        self.entities
            .splitters
            .get(&entity_id)
            .and_then(|state| state.next_output_by_lane.get(lane_index))
            .copied()
            .filter(|port| *port < 2)
            .unwrap_or(0)
    }

    fn advance_lane_items(&mut self, index: TransportLaneIndex, key: TransportLaneKey) -> bool {
        let Some((speed_subtiles_per_tick, mut items)) =
            take_lane_for_advancement(self.entities, key)
        else {
            return false;
        };
        let mut advanced_descending = SmallVec::<[BeltItem; 8]>::new();
        let mut downstream_item_position: Option<u16> = None;
        let mut lane_changed = false;

        while let Some(mut item) = items.pop() {
            let previous_position = item.position_subtile;
            let mut next_position = item.position_subtile + speed_subtiles_per_tick;
            if let Some(ahead_position) = downstream_item_position {
                next_position =
                    next_position.min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
            }

            if next_position >= BELT_SUBTILES_PER_TILE {
                let carried_position = next_position - BELT_SUBTILES_PER_TILE;
                item.position_subtile = carried_position;
                if self.try_route_carried_item(index, key, item) {
                    lane_changed = true;
                    continue;
                }

                item.position_subtile = BELT_SUBTILES_PER_TILE - 1;
            } else {
                item.position_subtile = next_position;
            }

            lane_changed |= item.position_subtile != previous_position;
            downstream_item_position = Some(item.position_subtile);
            advanced_descending.push(item);
        }

        advanced_descending.reverse();
        if lane_changed && !advanced_descending.is_empty() {
            self.active_lanes.mark_pending(index);
        }
        set_lane_items(self.entities, key, advanced_descending);
        if lane_changed {
            self.mark_items_changed(key.entity_id());
        }
        lane_changed
    }

    fn try_route_carried_item(
        &mut self,
        source_index: TransportLaneIndex,
        source_key: TransportLaneKey,
        item: BeltItem,
    ) -> bool {
        match source_key {
            TransportLaneKey::Belt { .. } => match self.graph.downstream_for(source_index) {
                TransportLaneDownstream::Belt {
                    downstream: Some(downstream),
                } => self.try_insert_carried_item(downstream, item),
                _ => false,
            },
            TransportLaneKey::Splitter { .. } => {
                self.try_route_splitter_item(source_index, source_key, item)
            }
        }
    }

    fn try_route_splitter_item(
        &mut self,
        index: TransportLaneIndex,
        key: TransportLaneKey,
        item: BeltItem,
    ) -> bool {
        let TransportLaneKey::Splitter {
            entity_id,
            lane_index,
            ..
        } = key
        else {
            return false;
        };
        let preferred = self.splitter_preferred_output_port(key);
        let TransportLaneDownstream::Splitter { outputs } = self.graph.downstream_for(index) else {
            return false;
        };

        for output_port in [preferred, 1 - preferred] {
            let Some(downstream) = outputs[output_port] else {
                continue;
            };

            if !self.try_insert_carried_item(downstream, item) {
                continue;
            }

            if output_port == preferred
                && let Some(state) = self.entities.splitters.get_mut(&entity_id)
            {
                state.next_output_by_lane[lane_index] = 1 - preferred;
            }
            return true;
        }

        false
    }

    fn try_insert_carried_item(&mut self, index: TransportLaneIndex, item: BeltItem) -> bool {
        if self.visit_state(index) == Some(BeltLaneVisitState::Processing) {
            return false;
        }

        let Some(key) = self.graph.key_for(index) else {
            return false;
        };
        let Some(lane) = lane_mut(self.entities, key) else {
            return false;
        };
        if !belt_lane_can_accept_position(lane, item.position_subtile) {
            return false;
        }

        insert_lane_item_at_entry(lane, item);
        self.mark_items_changed(key.entity_id());
        self.active_lanes.mark_pending(index);
        true
    }

    fn mark_items_changed(&mut self, entity_id: EntityId) {
        mark_item_revision(self.item_revision, self.item_revisions_by_entity, entity_id);
    }

    fn mark_upstream_lanes_active(&mut self, index: TransportLaneIndex) {
        for &upstream in self.graph.upstream_for(index) {
            self.active_lanes.mark_pending(upstream);
        }
    }

    fn visit_state(&self, index: TransportLaneIndex) -> Option<BeltLaneVisitState> {
        let state = self.visit_states.states.get(index.raw())?;
        if state.generation != self.visit_states.generation {
            return None;
        }
        match state.state {
            1 => Some(BeltLaneVisitState::Processing),
            2 => Some(BeltLaneVisitState::Done),
            _ => None,
        }
    }

    fn set_visit_state(&mut self, index: TransportLaneIndex, state: BeltLaneVisitState) {
        let Some(slot) = self.visit_states.states.get_mut(index.raw()) else {
            return;
        };
        slot.generation = self.visit_states.generation;
        slot.state = match state {
            BeltLaneVisitState::Processing => 1,
            BeltLaneVisitState::Done => 2,
        };
    }
}

pub(in crate::simulation) fn insert_lane_item_at_entry(lane: &mut BeltLane, item: BeltItem) {
    lane.items.push(item);
    // Belt lanes keep items sorted from upstream to downstream position.
    // Entry inserts are rare and lanes are short, so this keeps the invariant
    // simple without reintroducing front insertion.
    lane.items
        .sort_unstable_by_key(|item| item.position_subtile);
}
