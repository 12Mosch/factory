use super::types::TransportLaneKey;
use super::*;

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

pub(in crate::simulation::belt_ops) fn take_lane_for_advancement(
    entities: &mut EntityStore,
    key: TransportLaneKey,
) -> Option<(u16, SmallVec<[BeltItem; 8]>)> {
    let (speed_subtiles_per_tick, lane) = match key {
        TransportLaneKey::Belt {
            entity_id,
            lane_index,
        } => {
            let segment = entities.transport_belts.get_mut(&entity_id)?;
            (
                segment.speed_subtiles_per_tick,
                segment.lanes.get_mut(lane_index)?,
            )
        }
        TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        } => {
            let state = entities.splitters.get_mut(&entity_id)?;
            (
                state.speed_subtiles_per_tick,
                state.input_lanes.get_mut(input_port)?.get_mut(lane_index)?,
            )
        }
    };
    if lane.items.is_empty() {
        return None;
    }
    Some((speed_subtiles_per_tick, std::mem::take(&mut lane.items)))
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

pub(in crate::simulation) fn belt_lane_can_accept_position(
    lane: &BeltLane,
    position_subtile: u16,
) -> bool {
    let minimum_front_position = position_subtile.saturating_add(BELT_ITEM_SPACING_SUBTILES);
    lane.items
        .first()
        .is_none_or(|first| first.position_subtile >= minimum_front_position)
}
