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

pub(super) struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    visit_states: Vec<u8>,
}

impl<'a> TransportBeltAdvancement<'a> {
    pub(super) fn new(entities: &'a mut EntityStore) -> Self {
        let visit_state_len = (entities.next_entity_id as usize).saturating_mul(4);
        Self {
            entities,
            visit_states: vec![0; visit_state_len],
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
        match key {
            TransportLaneKey::Belt {
                entity_id,
                lane_index,
            } => {
                let mut downstream = SmallVec::new();
                if let Some(key) = self.belt_downstream_lane_key(entity_id, lane_index) {
                    downstream.push(key);
                }
                downstream
            }
            TransportLaneKey::Splitter {
                entity_id,
                lane_index,
                ..
            } => self.splitter_downstream_lane_keys(entity_id, lane_index),
        }
    }

    fn belt_downstream_lane_key(
        &self,
        entity_id: EntityId,
        lane_index: usize,
    ) -> Option<TransportLaneKey> {
        let placed = self.entities.placed_entities.get(&entity_id)?;
        let segment = self.entities.transport_belts.get(&entity_id)?;

        if underground_part(segment) == Some(UndergroundBeltPart::Entrance) {
            return self.paired_underground_exit_lane_key(placed, segment, lane_index);
        }

        let (dx, dy) = direction_tile_delta(segment.dir);
        let endpoint = self.transport_endpoint_at(placed.x + dx, placed.y + dy)?;

        Some(endpoint_lane_key(endpoint, lane_index))
    }

    fn paired_underground_exit_lane_key(
        &self,
        entrance_placed: &PlacedEntity,
        entrance_segment: &BeltSegment,
        lane_index: usize,
    ) -> Option<TransportLaneKey> {
        let entrance_underground = entrance_segment.underground?;
        let (dx, dy) = direction_tile_delta(entrance_segment.dir);
        let max_offset = i32::from(entrance_underground.max_distance) + 1;

        for offset in 1..=max_offset {
            let Some(TransportEndpoint::Belt { entity_id }) = self.transport_endpoint_at(
                entrance_placed.x + dx * offset,
                entrance_placed.y + dy * offset,
            ) else {
                continue;
            };
            let Some(exit_segment) = self.entities.transport_belts.get(&entity_id) else {
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

    fn splitter_downstream_lane_keys(
        &self,
        entity_id: EntityId,
        lane_index: usize,
    ) -> SmallVec<[TransportLaneKey; 2]> {
        let preferred = self
            .entities
            .splitters
            .get(&entity_id)
            .and_then(|state| state.next_output_by_lane.get(lane_index))
            .copied()
            .filter(|port| *port < 2)
            .unwrap_or(0);

        let mut downstream = SmallVec::new();
        for output_port in [preferred, 1 - preferred] {
            if let Some(key) = self.splitter_output_lane_key(entity_id, output_port, lane_index) {
                downstream.push(key);
            }
        }
        downstream
    }

    fn splitter_output_lane_key(
        &self,
        entity_id: EntityId,
        output_port: usize,
        lane_index: usize,
    ) -> Option<TransportLaneKey> {
        let placed = self.entities.placed_entities.get(&entity_id)?;
        let state = self.entities.splitters.get(&entity_id)?;
        let port_tile = splitter_port_tiles(placed)?[output_port];
        let (dx, dy) = direction_tile_delta(state.dir);
        let endpoint = self.transport_endpoint_at(port_tile.0 + dx, port_tile.1 + dy)?;

        Some(endpoint_lane_key(endpoint, lane_index))
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
            } => self
                .belt_downstream_lane_key(entity_id, lane_index)
                .is_some_and(|downstream| {
                    self.try_insert_carried_item(downstream, item_id, position_subtile)
                }),
            TransportLaneKey::Splitter {
                entity_id,
                lane_index,
                ..
            } => self.try_route_splitter_item(entity_id, lane_index, item_id, position_subtile),
        }
    }

    fn try_route_splitter_item(
        &mut self,
        entity_id: EntityId,
        lane_index: usize,
        item_id: ItemId,
        position_subtile: u16,
    ) -> bool {
        let preferred = self
            .entities
            .splitters
            .get(&entity_id)
            .and_then(|state| state.next_output_by_lane.get(lane_index))
            .copied()
            .filter(|port| *port < 2)
            .unwrap_or(0);

        for output_port in [preferred, 1 - preferred] {
            let Some(downstream) =
                self.splitter_output_lane_key(entity_id, output_port, lane_index)
            else {
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
        let state = *self.visit_states.get(visit_state_index(key)?)?;
        match state {
            1 => Some(BeltLaneVisitState::Processing),
            2 => Some(BeltLaneVisitState::Done),
            _ => None,
        }
    }

    fn set_visit_state(&mut self, key: TransportLaneKey, state: BeltLaneVisitState) {
        let Some(index) = visit_state_index(key) else {
            return;
        };
        let Some(slot) = self.visit_states.get_mut(index) else {
            return;
        };
        *slot = match state {
            BeltLaneVisitState::Processing => 1,
            BeltLaneVisitState::Done => 2,
        };
    }

    fn transport_endpoint_at(&self, x: i32, y: i32) -> Option<TransportEndpoint> {
        let entity_id = self.entities.occupancy.entity_at(x, y)?;
        if self.entities.transport_belts.contains_key(&entity_id) {
            return Some(TransportEndpoint::Belt { entity_id });
        }

        let placed = self.entities.placed_entities.get(&entity_id)?;
        if self.entities.splitters.contains_key(&entity_id) {
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
