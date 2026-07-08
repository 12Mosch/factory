use super::cache::{TransportLaneGraph, TransportLaneVisitStorage, visit_state_index};
use super::lane_access::{
    belt_lane_can_accept_position, lane_exists, lane_is_empty, lane_mut,
    lane_speed_subtiles_per_tick, set_lane_items, take_lane_items,
};
use super::types::{BeltLaneVisitState, TransportLaneDownstream, TransportLaneKey};
use super::*;

pub(in crate::simulation) struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    graph: &'a TransportLaneGraph,
    visit_states: &'a mut TransportLaneVisitStorage,
}

impl<'a> TransportBeltAdvancement<'a> {
    pub(in crate::simulation) fn new(
        entities: &'a mut EntityStore,
        graph: &'a TransportLaneGraph,
        visit_states: &'a mut TransportLaneVisitStorage,
    ) -> Self {
        Self {
            entities,
            graph,
            visit_states,
        }
    }

    pub(in crate::simulation) fn process_all_lanes(&mut self) {
        for index in 0..self.graph.lane_keys.len() {
            self.process_lane(self.graph.lane_keys[index]);
        }
    }

    fn process_lane(&mut self, key: TransportLaneKey) {
        match self.visit_state(key) {
            Some(BeltLaneVisitState::Done | BeltLaneVisitState::Processing) => return,
            None => {}
        }

        if !lane_exists(self.entities, key) {
            return;
        }

        self.set_visit_state(key, BeltLaneVisitState::Processing);

        let downstream = self.downstream_lane_keys(key);
        for downstream_key in &downstream {
            if self.visit_state(*downstream_key) != Some(BeltLaneVisitState::Processing) {
                self.process_lane(*downstream_key);
            }
        }

        self.advance_lane_items(key);
        self.set_visit_state(key, BeltLaneVisitState::Done);
    }

    fn downstream_lane_keys(&self, key: TransportLaneKey) -> SmallVec<[TransportLaneKey; 2]> {
        match self.graph.downstream_for(key) {
            TransportLaneDownstream::Missing => SmallVec::new(),
            TransportLaneDownstream::Belt { downstream } => {
                let mut downstream_keys = SmallVec::new();
                if let Some(key) = downstream {
                    downstream_keys.push(key);
                }
                downstream_keys
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

    fn advance_lane_items(&mut self, key: TransportLaneKey) {
        let Some(speed_subtiles_per_tick) = lane_speed_subtiles_per_tick(self.entities, key) else {
            return;
        };
        if lane_is_empty(self.entities, key) {
            return;
        }
        let Some(mut items) = take_lane_items(self.entities, key) else {
            return;
        };
        let mut advanced_descending = SmallVec::<[BeltItem; 8]>::new();
        let mut downstream_item_position: Option<u16> = None;

        while let Some(mut item) = items.pop() {
            let mut next_position = item.position_subtile + speed_subtiles_per_tick;
            if let Some(ahead_position) = downstream_item_position {
                next_position =
                    next_position.min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
            }

            if next_position >= BELT_SUBTILES_PER_TILE {
                let carried_position = next_position - BELT_SUBTILES_PER_TILE;
                if self.try_route_carried_item(key, item.item_id, carried_position) {
                    continue;
                }

                item.position_subtile = BELT_SUBTILES_PER_TILE - 1;
            } else {
                item.position_subtile = next_position;
            }

            downstream_item_position = Some(item.position_subtile);
            advanced_descending.push(item);
        }

        advanced_descending.reverse();
        set_lane_items(self.entities, key, advanced_descending);
    }

    fn try_route_carried_item(
        &mut self,
        source: TransportLaneKey,
        item_id: ItemId,
        position_subtile: u16,
    ) -> bool {
        match source {
            TransportLaneKey::Belt {
                entity_id,
                lane_index,
            } => {
                let key = TransportLaneKey::Belt {
                    entity_id,
                    lane_index,
                };
                match self.graph.downstream_for(key) {
                    TransportLaneDownstream::Belt {
                        downstream: Some(downstream),
                    } => self.try_insert_carried_item(downstream, item_id, position_subtile),
                    _ => false,
                }
            }
            TransportLaneKey::Splitter {
                entity_id,
                lane_index,
                input_port,
            } => {
                let key = TransportLaneKey::Splitter {
                    entity_id,
                    input_port,
                    lane_index,
                };
                self.try_route_splitter_item(key, item_id, position_subtile)
            }
        }
    }

    fn try_route_splitter_item(
        &mut self,
        key: TransportLaneKey,
        item_id: ItemId,
        position_subtile: u16,
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
        let TransportLaneDownstream::Splitter { outputs } = self.graph.downstream_for(key) else {
            return false;
        };

        for output_port in [preferred, 1 - preferred] {
            let Some(downstream) = outputs[output_port] else {
                continue;
            };

            if !self.try_insert_carried_item(downstream, item_id, position_subtile) {
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

    fn try_insert_carried_item(
        &mut self,
        key: TransportLaneKey,
        item_id: ItemId,
        position_subtile: u16,
    ) -> bool {
        if self.visit_state(key) == Some(BeltLaneVisitState::Processing) {
            return false;
        }

        let Some(lane) = lane_mut(self.entities, key) else {
            return false;
        };
        if !belt_lane_can_accept_position(lane, position_subtile) {
            return false;
        }

        lane.items.insert(
            0,
            BeltItem {
                item_id,
                position_subtile,
            },
        );
        true
    }

    fn visit_state(&self, key: TransportLaneKey) -> Option<BeltLaneVisitState> {
        let state = self.visit_states.states.get(visit_state_index(key)?)?;
        if state.generation != self.visit_states.generation {
            return None;
        }
        match state.state {
            1 => Some(BeltLaneVisitState::Processing),
            2 => Some(BeltLaneVisitState::Done),
            _ => None,
        }
    }

    fn set_visit_state(&mut self, key: TransportLaneKey, state: BeltLaneVisitState) {
        let Some(index) = visit_state_index(key) else {
            return;
        };
        let Some(slot) = self.visit_states.states.get_mut(index) else {
            return;
        };
        slot.generation = self.visit_states.generation;
        slot.state = match state {
            BeltLaneVisitState::Processing => 1,
            BeltLaneVisitState::Done => 2,
        };
    }
}
