use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum TransportLaneKey {
    Belt {
        entity_id: EntityId,
        lane_index: usize,
    },
    Splitter {
        entity_id: EntityId,
        input_port: usize,
        lane_index: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TransportEndpoint {
    Belt {
        entity_id: EntityId,
    },
    Splitter {
        entity_id: EntityId,
        input_port: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BeltLaneVisitState {
    Processing,
    Done,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(super) struct TransportLaneGraph {
    lane_keys: Vec<TransportLaneKey>,
    downstream_by_index: Vec<TransportLaneDownstream>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum TransportLaneDownstream {
    #[default]
    Missing,
    Belt {
        downstream: Option<TransportLaneKey>,
    },
    Splitter {
        outputs: [Option<TransportLaneKey>; 2],
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct TransportLaneVisitSlot {
    generation: u32,
    state: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(super) struct TransportLaneVisitStorage {
    generation: u32,
    states: Vec<TransportLaneVisitSlot>,
}

/// Subsystem-owned cache for belt/splitter transport.
///
/// This holds no authoritative simulation state: the durable belt/transport
/// data (lanes, item positions, splitter cursors) lives in [`EntityStore`].
/// The graph is a derived adjacency index rebuilt from `entities` whenever the
/// transport topology changes, and `visit_states` is per-tick DFS scratch.
/// All of it is `#[serde(skip)]` and reconstructed on load.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct TransportLaneCache {
    dirty: bool,
    pub(super) graph: TransportLaneGraph,
    pub(super) visit_states: TransportLaneVisitStorage,
    #[cfg(test)]
    rebuilds: u64,
}

impl Default for TransportLaneCache {
    fn default() -> Self {
        Self {
            dirty: true,
            graph: TransportLaneGraph::default(),
            visit_states: TransportLaneVisitStorage::default(),
            #[cfg(test)]
            rebuilds: 0,
        }
    }
}

impl TransportLaneCache {
    fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn refresh(&mut self, entities: &EntityStore) {
        if !self.dirty {
            return;
        }

        self.graph.rebuild(entities);
        self.dirty = false;
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
    }
}

impl TransportLaneGraph {
    pub(super) fn rebuild(&mut self, entities: &EntityStore) {
        let lane_count = (entities.next_entity_id as usize).saturating_mul(4);
        self.lane_keys.clear();
        self.lane_keys.reserve(
            entities
                .transport_belts
                .len()
                .saturating_mul(2)
                .saturating_add(entities.splitters.len().saturating_mul(4)),
        );
        self.downstream_by_index.clear();
        self.downstream_by_index
            .resize(lane_count, TransportLaneDownstream::Missing);

        for &entity_id in entities.transport_belts.keys() {
            for lane_index in 0..2 {
                let key = TransportLaneKey::Belt {
                    entity_id,
                    lane_index,
                };
                self.lane_keys.push(key);
                let Some(index) = visit_state_index(key) else {
                    continue;
                };
                if let Some(slot) = self.downstream_by_index.get_mut(index) {
                    *slot = TransportLaneDownstream::Belt {
                        downstream: belt_downstream_lane_key(entities, entity_id, lane_index),
                    };
                }
            }
        }

        for &entity_id in entities.splitters.keys() {
            for input_port in 0..2 {
                for lane_index in 0..2 {
                    let key = TransportLaneKey::Splitter {
                        entity_id,
                        input_port,
                        lane_index,
                    };
                    self.lane_keys.push(key);
                    let Some(index) = visit_state_index(key) else {
                        continue;
                    };
                    if let Some(slot) = self.downstream_by_index.get_mut(index) {
                        *slot = TransportLaneDownstream::Splitter {
                            outputs: [
                                splitter_output_lane_key(entities, entity_id, 0, lane_index),
                                splitter_output_lane_key(entities, entity_id, 1, lane_index),
                            ],
                        };
                    }
                }
            }
        }
    }

    fn downstream_for(&self, key: TransportLaneKey) -> TransportLaneDownstream {
        visit_state_index(key)
            .and_then(|index| self.downstream_by_index.get(index))
            .copied()
            .unwrap_or(TransportLaneDownstream::Missing)
    }
}

impl TransportLaneVisitStorage {
    pub(super) fn begin_tick(&mut self, required_len: usize) {
        if self.states.len() < required_len {
            self.states
                .resize(required_len, TransportLaneVisitSlot::default());
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.states.fill(TransportLaneVisitSlot::default());
            self.generation = 1;
        }
    }
}

impl Simulation {
    pub(super) fn prototype_affects_transport_lane_graph(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        (prototype.entity_kind == EntityKind::TransportBelt && prototype.transport_belt.is_some())
            || (prototype.entity_kind == EntityKind::Splitter && prototype.splitter.is_some())
    }

    pub(super) fn invalidate_transport_lane_graph(&mut self) {
        self.transport.invalidate();
    }

    pub(super) fn refresh_transport_lane_graph(&mut self) {
        self.transport.refresh(&self.entities);
    }

    #[cfg(test)]
    pub(super) fn transport_lane_graph_rebuild_count(&self) -> u64 {
        self.transport.rebuilds
    }
}

pub(super) struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    graph: &'a TransportLaneGraph,
    visit_states: &'a mut TransportLaneVisitStorage,
}

impl<'a> TransportBeltAdvancement<'a> {
    pub(super) fn new(
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

    pub(super) fn process_all_lanes(&mut self) {
        for index in 0..self.graph.lane_keys.len() {
            self.process_lane(self.graph.lane_keys[index]);
        }
    }

    pub(super) fn process_lane(&mut self, key: TransportLaneKey) {
        match self.visit_state(key) {
            Some(BeltLaneVisitState::Done | BeltLaneVisitState::Processing) => return,
            None => {}
        }

        if !self.lane_exists(key) {
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

    pub(super) fn downstream_lane_keys(
        &self,
        key: TransportLaneKey,
    ) -> SmallVec<[TransportLaneKey; 2]> {
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

    pub(super) fn advance_lane_items(&mut self, key: TransportLaneKey) {
        let Some(speed_subtiles_per_tick) = self.lane_speed_subtiles_per_tick(key) else {
            return;
        };
        if self.lane_is_empty(key) {
            return;
        }
        let Some(mut items) = self.take_lane_items(key) else {
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
        self.set_lane_items(key, advanced_descending);
    }

    fn lane_speed_subtiles_per_tick(&self, key: TransportLaneKey) -> Option<u16> {
        match key {
            TransportLaneKey::Belt { entity_id, .. } => self
                .entities
                .transport_belts
                .get(&entity_id)
                .map(|segment| segment.speed_subtiles_per_tick),
            TransportLaneKey::Splitter { entity_id, .. } => self
                .entities
                .splitters
                .get(&entity_id)
                .map(|state| state.speed_subtiles_per_tick),
        }
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

    pub(super) fn try_insert_carried_item(
        &mut self,
        key: TransportLaneKey,
        item_id: ItemId,
        position_subtile: u16,
    ) -> bool {
        if self.visit_state(key) == Some(BeltLaneVisitState::Processing) {
            return false;
        }

        let Some(lane) = self.lane_mut(key) else {
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

    fn lane_exists(&self, key: TransportLaneKey) -> bool {
        match key {
            TransportLaneKey::Belt {
                entity_id,
                lane_index,
            } => self
                .entities
                .transport_belts
                .get(&entity_id)
                .is_some_and(|segment| lane_index < segment.lanes.len()),
            TransportLaneKey::Splitter {
                entity_id,
                input_port,
                lane_index,
            } => self
                .entities
                .splitters
                .get(&entity_id)
                .is_some_and(|state| {
                    input_port < state.input_lanes.len()
                        && lane_index < state.input_lanes[input_port].len()
                }),
        }
    }

    fn lane_is_empty(&self, key: TransportLaneKey) -> bool {
        match key {
            TransportLaneKey::Belt {
                entity_id,
                lane_index,
            } => self
                .entities
                .transport_belts
                .get(&entity_id)
                .and_then(|segment| segment.lanes.get(lane_index))
                .is_none_or(|lane| lane.items.is_empty()),
            TransportLaneKey::Splitter {
                entity_id,
                input_port,
                lane_index,
            } => self
                .entities
                .splitters
                .get(&entity_id)
                .and_then(|state| state.input_lanes.get(input_port))
                .and_then(|lanes| lanes.get(lane_index))
                .is_none_or(|lane| lane.items.is_empty()),
        }
    }

    fn lane_mut(&mut self, key: TransportLaneKey) -> Option<&mut BeltLane> {
        match key {
            TransportLaneKey::Belt {
                entity_id,
                lane_index,
            } => self
                .entities
                .transport_belts
                .get_mut(&entity_id)?
                .lanes
                .get_mut(lane_index),
            TransportLaneKey::Splitter {
                entity_id,
                input_port,
                lane_index,
            } => self
                .entities
                .splitters
                .get_mut(&entity_id)?
                .input_lanes
                .get_mut(input_port)?
                .get_mut(lane_index),
        }
    }

    fn take_lane_items(&mut self, key: TransportLaneKey) -> Option<SmallVec<[BeltItem; 8]>> {
        self.lane_mut(key)
            .map(|lane| std::mem::take(&mut lane.items))
    }

    fn set_lane_items(&mut self, key: TransportLaneKey, items: SmallVec<[BeltItem; 8]>) {
        if let Some(lane) = self.lane_mut(key) {
            lane.items = items;
        }
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

fn visit_state_index(key: TransportLaneKey) -> Option<usize> {
    let (entity_id, lane_offset) = match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => (entity_id, lane_index),
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => (
            entity_id,
            input_port.checked_mul(2)?.checked_add(lane_index)?,
        ),
    };
    let entity_index = usize::try_from(entity_id.raw()).ok()?;
    entity_index.checked_mul(4)?.checked_add(lane_offset)
}

pub(super) fn splitter_port_tiles(placed: &PlacedEntity) -> Option<[(i32, i32); 2]> {
    let mut tiles = placed.footprint.tiles();
    if tiles.len() != 2 {
        return None;
    }

    tiles.sort_unstable();
    Some([tiles[0], tiles[1]])
}

pub(super) fn belt_lane_can_accept_position(lane: &BeltLane, position_subtile: u16) -> bool {
    lane.items
        .first()
        .is_none_or(|first| first.position_subtile >= position_subtile + BELT_ITEM_SPACING_SUBTILES)
}

fn endpoint_lane_key(endpoint: TransportEndpoint, lane_index: usize) -> TransportLaneKey {
    match endpoint {
        TransportEndpoint::Belt { entity_id } => TransportLaneKey::Belt {
            entity_id,
            lane_index,
        },
        TransportEndpoint::Splitter {
            entity_id,
            input_port,
        } => TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        },
    }
}

fn belt_downstream_lane_key(
    entities: &EntityStore,
    entity_id: EntityId,
    lane_index: usize,
) -> Option<TransportLaneKey> {
    let placed = entities.placed_entities.get(&entity_id)?;
    let segment = entities.transport_belts.get(&entity_id)?;

    if underground_part(segment) == Some(UndergroundBeltPart::Entrance) {
        return paired_underground_exit_lane_key(entities, placed, segment, lane_index);
    }

    let (dx, dy) = direction_tile_delta(segment.dir);
    let endpoint = transport_endpoint_at(entities, placed.x + dx, placed.y + dy)?;

    Some(endpoint_lane_key(endpoint, lane_index))
}

fn paired_underground_exit_lane_key(
    entities: &EntityStore,
    entrance_placed: &PlacedEntity,
    entrance_segment: &BeltSegment,
    lane_index: usize,
) -> Option<TransportLaneKey> {
    let entrance_underground = entrance_segment.underground?;
    let (dx, dy) = direction_tile_delta(entrance_segment.dir);
    let max_offset = i32::from(entrance_underground.max_distance) + 1;

    for offset in 1..=max_offset {
        let Some(TransportEndpoint::Belt { entity_id }) = transport_endpoint_at(
            entities,
            entrance_placed.x + dx * offset,
            entrance_placed.y + dy * offset,
        ) else {
            continue;
        };
        let Some(exit_segment) = entities.transport_belts.get(&entity_id) else {
            continue;
        };
        let underground_distance = (offset - 1) as u8;

        if is_valid_underground_pair(entrance_segment, exit_segment, underground_distance) {
            return Some(TransportLaneKey::Belt {
                entity_id,
                lane_index,
            });
        }
    }

    None
}

fn splitter_output_lane_key(
    entities: &EntityStore,
    entity_id: EntityId,
    output_port: usize,
    lane_index: usize,
) -> Option<TransportLaneKey> {
    let placed = entities.placed_entities.get(&entity_id)?;
    let state = entities.splitters.get(&entity_id)?;
    let port_tile = splitter_port_tiles(placed)?.get(output_port).copied()?;
    let (dx, dy) = direction_tile_delta(state.dir);
    let endpoint = transport_endpoint_at(entities, port_tile.0 + dx, port_tile.1 + dy)?;

    Some(endpoint_lane_key(endpoint, lane_index))
}

fn transport_endpoint_at(entities: &EntityStore, x: i32, y: i32) -> Option<TransportEndpoint> {
    let entity_id = entities.occupancy.entity_at(x, y)?;
    if entities.transport_belts.contains_key(&entity_id) {
        return Some(TransportEndpoint::Belt { entity_id });
    }

    let placed = entities.placed_entities.get(&entity_id)?;
    if entities.splitters.contains_key(&entity_id) {
        let input_port = splitter_port_tiles(placed)?
            .into_iter()
            .position(|tile| tile == (x, y))?;
        return Some(TransportEndpoint::Splitter {
            entity_id,
            input_port,
        });
    }

    None
}

fn is_valid_underground_pair(
    entrance: &BeltSegment,
    exit: &BeltSegment,
    underground_distance: u8,
) -> bool {
    let Some(entrance_underground) = entrance.underground else {
        return false;
    };
    let Some(exit_underground) = exit.underground else {
        return false;
    };

    entrance_underground.part == UndergroundBeltPart::Entrance
        && exit_underground.part == UndergroundBeltPart::Exit
        && entrance.dir == exit.dir
        && underground_distance <= entrance_underground.max_distance
        && underground_distance <= exit_underground.max_distance
}

fn underground_part(segment: &BeltSegment) -> Option<UndergroundBeltPart> {
    segment
        .underground
        .as_ref()
        .map(|underground| underground.part)
}

pub(super) fn direction_tile_delta(direction: Direction) -> (i32, i32) {
    match direction {
        Direction::North => (0, 1),
        Direction::East => (1, 0),
        Direction::South => (0, -1),
        Direction::West => (-1, 0),
    }
}
