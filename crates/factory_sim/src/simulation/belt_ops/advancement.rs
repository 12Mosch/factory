use super::cache::mark_item_revision;
use super::cache::{TransportLaneGraph, TransportRunActiveStorage, TransportRunVisitStorage};
use super::lane_access::{belt_lane_can_accept_position, lane_mut};
use super::types::{
    TransportLaneDownstream, TransportLaneIndex, TransportLaneKey, TransportRunIndex,
    TransportRunTraversalStep, TransportRunVisitState,
};
use super::*;

/// Where items that cross a lane's tile boundary continue.
#[derive(Clone, Copy)]
enum LaneRouting {
    /// Cyclic-run tail: the run's own head has not advanced yet this tick,
    /// so carried items wait at the tile edge.
    Blocked,
    /// The next lane in the same run. It already advanced this tick, so
    /// inserts need no visit-state check.
    Successor { key: TransportLaneKey },
    /// The lane ends its run; resolve downstream through the graph and
    /// respect run visit states so carries into runs still on the traversal
    /// stack (cycles across runs) wait a tick.
    Graph { downstream: TransportLaneDownstream },
}

pub(in crate::simulation) struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    graph: &'a TransportLaneGraph,
    visit_states: &'a mut TransportRunVisitStorage,
    active_runs: &'a mut TransportRunActiveStorage,
    item_revision: &'a mut u64,
    item_revisions_by_entity: &'a mut Vec<u64>,
}

impl<'a> TransportBeltAdvancement<'a> {
    pub(in crate::simulation) fn new(
        entities: &'a mut EntityStore,
        graph: &'a TransportLaneGraph,
        visit_states: &'a mut TransportRunVisitStorage,
        active_runs: &'a mut TransportRunActiveStorage,
        item_revision: &'a mut u64,
        item_revisions_by_entity: &'a mut Vec<u64>,
    ) -> Self {
        Self {
            entities,
            graph,
            visit_states,
            active_runs,
            item_revision,
            item_revisions_by_entity,
        }
    }

    pub(in crate::simulation) fn process_active_runs(&mut self) {
        let mut traversal_stack = std::mem::take(&mut self.visit_states.traversal_stack);
        for index in 0..self.active_runs.runs.len() {
            let run = self.active_runs.runs[index];
            let start_position = self.active_runs.active_start_position(run);
            self.process_run(run, start_position, &mut traversal_stack);
        }
        self.visit_states.traversal_stack = traversal_stack;
    }

    /// Post-order traversal over runs: downstream runs advance first so their
    /// entry positions are free before upstream runs try to carry items over.
    fn process_run(
        &mut self,
        root: TransportRunIndex,
        root_start_position: usize,
        traversal_stack: &mut Vec<TransportRunTraversalStep>,
    ) {
        debug_assert!(traversal_stack.is_empty());
        traversal_stack.push(TransportRunTraversalStep::Enter(root));

        while let Some(step) = traversal_stack.pop() {
            match step {
                TransportRunTraversalStep::Enter(run) => {
                    if self.visit_state(run).is_some() {
                        continue;
                    }
                    self.set_visit_state(run, TransportRunVisitState::Processing);
                    let Some((tail_key, downstream_slots)) = self.tail_routing(run) else {
                        self.set_visit_state(run, TransportRunVisitState::Done);
                        continue;
                    };
                    traversal_stack.push(TransportRunTraversalStep::Exit { run, tail_key });
                    for slot in downstream_slots.into_iter().rev() {
                        let Some(downstream_run) = self.graph.run_for_slot(slot) else {
                            continue;
                        };
                        if self.visit_state(downstream_run)
                            != Some(TransportRunVisitState::Processing)
                        {
                            traversal_stack.push(TransportRunTraversalStep::Enter(downstream_run));
                        }
                    }
                }
                TransportRunTraversalStep::Exit { run, tail_key } => {
                    let start_position = if run == root {
                        root_start_position
                    } else {
                        self.active_runs.active_start_position(run)
                    };
                    self.advance_run(run, start_position, tail_key);
                    self.set_visit_state(run, TransportRunVisitState::Done);
                }
            }
        }
    }

    fn tail_routing(
        &self,
        run: TransportRunIndex,
    ) -> Option<(TransportLaneKey, SmallVec<[TransportLaneIndex; 2]>)> {
        let lanes = self.graph.run_lanes(run);
        let &tail = lanes.last()?;
        let record = self.graph.lane(tail)?;
        let key = record.key?;
        Some((key, self.downstream_lane_indices(record.downstream, key)))
    }

    fn downstream_lane_indices(
        &self,
        downstream_record: TransportLaneDownstream,
        key: TransportLaneKey,
    ) -> SmallVec<[TransportLaneIndex; 2]> {
        match downstream_record {
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

    /// Advances every lane of `run` from its downstream end to its head, so
    /// each lane's carried items enter a successor that has already moved.
    fn advance_run(
        &mut self,
        run: TransportRunIndex,
        start_position: usize,
        tail_key: TransportLaneKey,
    ) {
        let graph = self.graph;
        let lanes = graph.run_lanes(run);
        let cyclic = graph.run_is_cyclic(run);
        let mut any_changed = false;
        let mut boundary_changed = false;

        for (position, &slot) in lanes.iter().enumerate().skip(start_position).rev() {
            let Some(record) = graph.lane(slot) else {
                continue;
            };
            let Some(mut key) = record.key else {
                continue;
            };
            if position + 1 == lanes.len() {
                key = tail_key;
            }
            let routing = match lanes.get(position + 1) {
                Some(&successor) => match graph.lane(successor).and_then(|lane| lane.key) {
                    Some(successor_key) => LaneRouting::Successor { key: successor_key },
                    None => LaneRouting::Blocked,
                },
                None if cyclic => LaneRouting::Blocked,
                None => LaneRouting::Graph {
                    downstream: record.downstream,
                },
            };
            let changed = self.advance_lane_items(key, record.speed_subtiles_per_tick, routing);
            any_changed |= changed;
            if position == start_position {
                boundary_changed = changed;
            }
        }

        if !any_changed {
            return;
        }
        self.active_runs.mark_pending(run, start_position);
        if !boundary_changed {
            return;
        }
        if start_position > 0 {
            self.active_runs.mark_pending(run, start_position - 1);
        } else if let Some(&head) = lanes.first() {
            // Only the run head can have feeders outside the run.
            for &upstream in graph.upstream_for(head) {
                if let Some((upstream_run, position)) = graph.run_and_position_for_slot(upstream) {
                    self.active_runs.mark_pending(upstream_run, position);
                }
            }
        }
    }

    fn advance_lane_items(
        &mut self,
        key: TransportLaneKey,
        speed_subtiles_per_tick: u16,
        routing: LaneRouting,
    ) -> bool {
        let Some(lane) = lane_mut(self.entities, key) else {
            return false;
        };
        let Some(front) = lane.items.last() else {
            return false;
        };

        // Hot path: the front item stays inside the tile this tick, so no item
        // can cross into another entity and the lane can advance in place
        // through the one lookup above.
        if front
            .position_subtile
            .saturating_add(speed_subtiles_per_tick)
            < BELT_SUBTILES_PER_TILE
        {
            let mut lane_changed = false;
            let mut downstream_item_position: Option<u16> = None;
            for item in lane.items.iter_mut().rev() {
                let mut next_position = item.position_subtile + speed_subtiles_per_tick;
                if let Some(ahead_position) = downstream_item_position {
                    next_position = next_position
                        .min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
                }
                lane_changed |= next_position != item.position_subtile;
                item.position_subtile = next_position;
                downstream_item_position = Some(next_position);
            }
            if lane_changed {
                self.mark_items_changed(key.entity_id());
            }
            return lane_changed;
        }

        self.advance_lane_items_with_crossing(key, speed_subtiles_per_tick, routing)
    }

    /// Slow path for lanes whose front item may leave the tile. Front items
    /// are routed one at a time, re-borrowing the lane between routing calls;
    /// everything that stays advances in place afterwards. Routing targets a
    /// different entity than the source lane, so the repeated lookups stay in
    /// cache instead of paying for a take-and-write-back of the item buffer.
    fn advance_lane_items_with_crossing(
        &mut self,
        key: TransportLaneKey,
        speed_subtiles_per_tick: u16,
        routing: LaneRouting,
    ) -> bool {
        let mut lane_changed = false;
        let mut downstream_item_position: Option<u16> = None;
        let mut front_settled = false;

        loop {
            let crossing = {
                let Some(lane) = lane_mut(self.entities, key) else {
                    return lane_changed;
                };
                let Some(&front) = lane.items.last() else {
                    break;
                };
                let mut next_position = front.position_subtile + speed_subtiles_per_tick;
                if let Some(ahead_position) = downstream_item_position {
                    next_position = next_position
                        .min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
                }
                (next_position >= BELT_SUBTILES_PER_TILE).then(|| {
                    let mut item = front;
                    item.position_subtile = next_position - BELT_SUBTILES_PER_TILE;
                    item
                })
            };

            let Some(item) = crossing else {
                break;
            };
            if self.try_route_carried_item(key, routing, item) {
                lane_changed = true;
                if let Some(lane) = lane_mut(self.entities, key) {
                    lane.items.pop();
                }
                continue;
            }

            // Routing failed: the front waits at the tile edge and shields
            // the items behind it.
            if let Some(lane) = lane_mut(self.entities, key)
                && let Some(front) = lane.items.last_mut()
            {
                lane_changed |= front.position_subtile != BELT_SUBTILES_PER_TILE - 1;
                front.position_subtile = BELT_SUBTILES_PER_TILE - 1;
            }
            downstream_item_position = Some(BELT_SUBTILES_PER_TILE - 1);
            front_settled = true;
            break;
        }

        if let Some(lane) = lane_mut(self.entities, key) {
            let settled = usize::from(front_settled);
            let unresolved = lane.items.len() - settled;
            for item in lane.items[..unresolved].iter_mut().rev() {
                let mut next_position = item.position_subtile + speed_subtiles_per_tick;
                if let Some(ahead_position) = downstream_item_position {
                    next_position = next_position
                        .min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
                }
                lane_changed |= next_position != item.position_subtile;
                item.position_subtile = next_position;
                downstream_item_position = Some(next_position);
            }
        }

        if lane_changed {
            self.mark_items_changed(key.entity_id());
        }
        lane_changed
    }

    fn try_route_carried_item(
        &mut self,
        source_key: TransportLaneKey,
        routing: LaneRouting,
        item: BeltItem,
    ) -> bool {
        match routing {
            LaneRouting::Blocked => false,
            LaneRouting::Successor { key } => self.try_insert_lane_item(key, item),
            LaneRouting::Graph { downstream } => match source_key {
                TransportLaneKey::Belt { .. } => match downstream {
                    TransportLaneDownstream::Belt {
                        downstream: Some(downstream),
                    } => self.try_insert_carried_item(downstream, item),
                    _ => false,
                },
                TransportLaneKey::Splitter { .. } => {
                    self.try_route_splitter_item(downstream, source_key, item)
                }
            },
        }
    }

    fn try_route_splitter_item(
        &mut self,
        downstream: TransportLaneDownstream,
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
        let TransportLaneDownstream::Splitter { outputs } = downstream else {
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
        let Some((target_run, target_position)) = self.graph.run_and_position_for_slot(index)
        else {
            return false;
        };
        if self.visit_state(target_run) == Some(TransportRunVisitState::Processing) {
            return false;
        }
        let Some(key) = self.graph.key_for(index) else {
            return false;
        };
        if !self.try_insert_lane_item(key, item) {
            return false;
        }
        self.active_runs.mark_pending(target_run, target_position);
        true
    }

    fn try_insert_lane_item(&mut self, key: TransportLaneKey, item: BeltItem) -> bool {
        let Some(lane) = lane_mut(self.entities, key) else {
            return false;
        };
        if !belt_lane_can_accept_position(lane, item.position_subtile) {
            return false;
        }

        insert_lane_item_at_entry(lane, item);
        self.mark_items_changed(key.entity_id());
        true
    }

    fn mark_items_changed(&mut self, entity_id: EntityId) {
        mark_item_revision(self.item_revision, self.item_revisions_by_entity, entity_id);
    }

    fn visit_state(&self, run: TransportRunIndex) -> Option<TransportRunVisitState> {
        let state = self.visit_states.states.get(run.raw())?;
        if state.generation != self.visit_states.generation {
            return None;
        }
        match state.state {
            1 => Some(TransportRunVisitState::Processing),
            2 => Some(TransportRunVisitState::Done),
            _ => None,
        }
    }

    fn set_visit_state(&mut self, run: TransportRunIndex, state: TransportRunVisitState) {
        let Some(slot) = self.visit_states.states.get_mut(run.raw()) else {
            return;
        };
        slot.generation = self.visit_states.generation;
        slot.state = match state {
            TransportRunVisitState::Processing => 1,
            TransportRunVisitState::Done => 2,
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
