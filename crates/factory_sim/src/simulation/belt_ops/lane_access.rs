use super::types::TransportLaneKey;
use super::*;

pub(in crate::simulation::belt_ops) fn lane_exists(
    entities: &EntityStore,
    key: TransportLaneKey,
) -> bool {
    match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => entities
            .transport_belts
            .get(&entity_id)
            .is_some_and(|segment| lane_index < segment.lanes.len()),
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => entities.splitters.get(&entity_id).is_some_and(|state| {
            input_port < state.input_lanes.len() && lane_index < state.input_lanes[input_port].len()
        }),
    }
}

pub(in crate::simulation::belt_ops) fn lane_is_empty(
    entities: &EntityStore,
    key: TransportLaneKey,
) -> bool {
    match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => entities
            .transport_belts
            .get(&entity_id)
            .and_then(|segment| segment.lanes.get(lane_index))
            .is_none_or(|lane| lane.items.is_empty()),
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => entities
            .splitters
            .get(&entity_id)
            .and_then(|state| state.input_lanes.get(input_port))
            .and_then(|lanes| lanes.get(lane_index))
            .is_none_or(|lane| lane.items.is_empty()),
    }
}

pub(in crate::simulation::belt_ops) fn lane_mut(
    entities: &mut EntityStore,
    key: TransportLaneKey,
) -> Option<&mut BeltLane> {
    match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => entities
            .transport_belts
            .get_mut(&entity_id)?
            .lanes
            .get_mut(lane_index),
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => entities
            .splitters
            .get_mut(&entity_id)?
            .input_lanes
            .get_mut(input_port)?
            .get_mut(lane_index),
    }
}

pub(in crate::simulation::belt_ops) fn take_lane_items(
    entities: &mut EntityStore,
    key: TransportLaneKey,
) -> Option<SmallVec<[BeltItem; 8]>> {
    lane_mut(entities, key).map(|lane| std::mem::take(&mut lane.items))
}

pub(in crate::simulation::belt_ops) fn set_lane_items(
    entities: &mut EntityStore,
    key: TransportLaneKey,
    items: SmallVec<[BeltItem; 8]>,
) {
    if let Some(lane) = lane_mut(entities, key) {
        lane.items = items;
    }
}

pub(in crate::simulation::belt_ops) fn lane_speed_subtiles_per_tick(
    entities: &EntityStore,
    key: TransportLaneKey,
) -> Option<u16> {
    match key {
        TransportLaneKey::Belt { entity_id, .. } => entities
            .transport_belts
            .get(&entity_id)
            .map(|segment| segment.speed_subtiles_per_tick),
        TransportLaneKey::Splitter { entity_id, .. } => entities
            .splitters
            .get(&entity_id)
            .map(|state| state.speed_subtiles_per_tick),
    }
}

pub(in crate::simulation) fn belt_lane_can_accept_position(
    lane: &BeltLane,
    position_subtile: u16,
) -> bool {
    let minimum_front_position = position_subtile.saturating_add(BELT_ITEM_SPACING_SUBTILES);
    lane.items
        .first()
        .is_none_or(|first| first.position_subtile >= minimum_front_position)
}
