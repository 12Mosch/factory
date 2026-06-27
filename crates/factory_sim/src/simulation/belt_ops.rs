use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BeltLaneKey {
    pub(super) entity_id: EntityId,
    pub(super) lane_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BeltLaneVisitState {
    Processing,
    Done,
}

pub(super) struct TransportBeltAdvancement<'a> {
    entities: &'a mut EntityStore,
    tile_to_belt: BTreeMap<(i32, i32), EntityId>,
    visit_states: BTreeMap<BeltLaneKey, BeltLaneVisitState>,
}

impl<'a> TransportBeltAdvancement<'a> {
    pub(super) fn new(
        entities: &'a mut EntityStore,
        tile_to_belt: BTreeMap<(i32, i32), EntityId>,
    ) -> Self {
        Self {
            entities,
            tile_to_belt,
            visit_states: BTreeMap::new(),
        }
    }

    pub(super) fn process_lane(&mut self, key: BeltLaneKey) {
        match self.visit_states.get(&key).copied() {
            Some(BeltLaneVisitState::Done | BeltLaneVisitState::Processing) => return,
            None => {}
        }

        if !self.entities.transport_belts.contains_key(&key.entity_id) {
            return;
        }

        self.visit_states
            .insert(key, BeltLaneVisitState::Processing);

        let downstream = self.downstream_lane_key(key);
        if let Some(downstream) = downstream
            && self.visit_states.get(&downstream) != Some(&BeltLaneVisitState::Processing)
        {
            self.process_lane(downstream);
        }

        self.advance_lane_items(key, downstream);
        self.visit_states.insert(key, BeltLaneVisitState::Done);
    }

    pub(super) fn downstream_lane_key(&self, key: BeltLaneKey) -> Option<BeltLaneKey> {
        let placed = self.entities.placed_entities.get(&key.entity_id)?;
        let segment = self.entities.transport_belts.get(&key.entity_id)?;
        let (dx, dy) = direction_tile_delta(segment.dir);
        let next_entity_id = self.tile_to_belt.get(&(placed.x + dx, placed.y + dy))?;

        Some(BeltLaneKey {
            entity_id: *next_entity_id,
            lane_index: key.lane_index,
        })
    }

    pub(super) fn advance_lane_items(&mut self, key: BeltLaneKey, downstream: Option<BeltLaneKey>) {
        let mut items = {
            let segment = self
                .entities
                .transport_belts
                .get_mut(&key.entity_id)
                .expect("lane processing validated belt existence");
            std::mem::take(&mut segment.lanes[key.lane_index].items)
        };
        let mut advanced_descending = Vec::with_capacity(items.len());
        let mut downstream_item_position: Option<u16> = None;

        while let Some(mut item) = items.pop() {
            let mut next_position = item.position_subtile + BASIC_BELT_SPEED_SUBTILES_PER_TICK;
            if let Some(ahead_position) = downstream_item_position {
                next_position =
                    next_position.min(ahead_position.saturating_sub(BELT_ITEM_SPACING_SUBTILES));
            }

            if next_position >= BELT_SUBTILES_PER_TILE {
                let carried_position = next_position - BELT_SUBTILES_PER_TILE;
                if let Some(downstream) = downstream
                    && self.try_insert_carried_item(downstream, item.item_id, carried_position)
                {
                    continue;
                }

                item.position_subtile = BELT_SUBTILES_PER_TILE - 1;
            } else {
                item.position_subtile = next_position;
            }

            downstream_item_position = Some(item.position_subtile);
            advanced_descending.push(item);
        }

        let segment = self
            .entities
            .transport_belts
            .get_mut(&key.entity_id)
            .expect("lane processing validated belt existence");
        let lane = &mut segment.lanes[key.lane_index];
        lane.items = advanced_descending.into_iter().rev().collect();
    }

    pub(super) fn try_insert_carried_item(
        &mut self,
        key: BeltLaneKey,
        item_id: ItemId,
        position_subtile: u16,
    ) -> bool {
        if self.visit_states.get(&key) == Some(&BeltLaneVisitState::Processing) {
            return false;
        }

        let Some(segment) = self.entities.transport_belts.get_mut(&key.entity_id) else {
            return false;
        };
        let lane = &mut segment.lanes[key.lane_index];
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
}

pub(super) fn transport_belt_tile_map(entities: &EntityStore) -> BTreeMap<(i32, i32), EntityId> {
    entities
        .transport_belts
        .keys()
        .filter_map(|entity_id| {
            entities
                .placed_entities
                .get(entity_id)
                .map(|placed| ((placed.x, placed.y), *entity_id))
        })
        .collect()
}

pub(super) fn belt_lane_can_accept_position(lane: &BeltLane, position_subtile: u16) -> bool {
    lane.items
        .first()
        .is_none_or(|first| first.position_subtile >= position_subtile + BELT_ITEM_SPACING_SUBTILES)
}

pub(super) fn direction_tile_delta(direction: Direction) -> (i32, i32) {
    match direction {
        Direction::North => (0, 1),
        Direction::East => (1, 0),
        Direction::South => (0, -1),
        Direction::West => (-1, 0),
    }
}
