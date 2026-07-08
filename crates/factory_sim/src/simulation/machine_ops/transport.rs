use crate::simulation::*;

pub(in crate::simulation) fn belt_pickup_item(segment: &BeltSegment) -> Option<ItemId> {
    segment
        .lanes
        .iter()
        .flat_map(|lane| lane.items.iter())
        .max_by_key(|item| item.position_subtile)
        .map(|item| item.item_id)
}

pub(in crate::simulation) fn remove_one_item_from_belt(
    segment: &mut BeltSegment,
    item_id: ItemId,
) -> Option<usize> {
    let (lane_index, item_index, _) = segment
        .lanes
        .iter()
        .enumerate()
        .flat_map(|(lane_index, lane)| {
            lane.items
                .iter()
                .enumerate()
                .map(move |(item_index, item)| (lane_index, item_index, item))
        })
        .filter(|(_, _, item)| item.item_id == item_id)
        .max_by_key(|(_, _, item)| item.position_subtile)?;

    segment.lanes[lane_index].items.remove(item_index);
    Some(lane_index)
}

pub(in crate::simulation) fn splitter_pickup_item(state: &SplitterState) -> Option<ItemId> {
    state
        .input_lanes
        .iter()
        .flat_map(|input_lanes| input_lanes.iter())
        .flat_map(|lane| lane.items.iter())
        .max_by_key(|item| item.position_subtile)
        .map(|item| item.item_id)
}

pub(in crate::simulation) fn remove_one_item_from_splitter(
    state: &mut SplitterState,
    item_id: ItemId,
) -> Option<(usize, usize)> {
    let (input_port, lane_index, item_index, _) = state
        .input_lanes
        .iter()
        .enumerate()
        .flat_map(|(input_port, input_lanes)| {
            input_lanes
                .iter()
                .enumerate()
                .flat_map(move |(lane_index, lane)| {
                    lane.items
                        .iter()
                        .enumerate()
                        .map(move |(item_index, item)| (input_port, lane_index, item_index, item))
                })
        })
        .filter(|(_, _, _, item)| item.item_id == item_id)
        .max_by_key(|(_, _, _, item)| item.position_subtile)?;

    state.input_lanes[input_port][lane_index]
        .items
        .remove(item_index);
    Some((input_port, lane_index))
}

pub(in crate::simulation) fn belt_output_lane_index(
    segment: &BeltSegment,
    _item_id: ItemId,
) -> Option<usize> {
    if belt_lane_can_accept_position(&segment.lanes[0], 0) {
        Some(0)
    } else if belt_lane_can_accept_position(&segment.lanes[1], 0) {
        Some(1)
    } else {
        None
    }
}

pub(in crate::simulation) fn splitter_output_lane_index(
    state: &SplitterState,
    input_port: usize,
    _item_id: ItemId,
) -> Option<usize> {
    let input_lanes = state.input_lanes.get(input_port)?;
    if belt_lane_can_accept_position(&input_lanes[0], 0) {
        Some(0)
    } else if belt_lane_can_accept_position(&input_lanes[1], 0) {
        Some(1)
    } else {
        None
    }
}

pub(in crate::simulation) fn splitter_input_port_for_occupied_tile(
    entities: &EntityStore,
    entity_id: EntityId,
    tile: (i32, i32),
) -> Option<usize> {
    let placed = entities.placed_entities.get(&entity_id)?;
    splitter_port_tiles(placed)?
        .into_iter()
        .position(|port_tile| port_tile == tile)
}
