use super::types::{TransportEndpoint, TransportLaneKey};
use super::*;

pub(in crate::simulation) fn splitter_port_tiles(
    placed: &PlacedEntity,
) -> Option<[(WorldTileCoord, WorldTileCoord); 2]> {
    let mut tiles = placed.footprint.tiles();
    if tiles.len() != 2 {
        return None;
    }

    tiles.sort_unstable();
    Some([tiles[0], tiles[1]])
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

pub(in crate::simulation::belt_ops) fn belt_downstream_lane_key(
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
    let endpoint =
        transport_endpoint_at(entities, placed.x + i64::from(dx), placed.y + i64::from(dy))?;

    Some(endpoint_lane_key(endpoint, lane_index))
}

fn paired_underground_exit_lane_key(
    entities: &EntityStore,
    entrance_placed: &PlacedEntity,
    entrance_segment: &BeltSegment,
    lane_index: usize,
) -> Option<TransportLaneKey> {
    let entrance_underground = entrance_segment.underground?;
    let entity_id = paired_underground_entity(
        entities,
        entrance_placed,
        UndergroundEndpoint {
            part: entrance_underground.part,
            max_distance: entrance_underground.max_distance,
        },
        |candidate_id| {
            let underground = entities.transport_belts.get(&candidate_id)?.underground?;
            Some(UndergroundEndpoint {
                part: underground.part,
                max_distance: underground.max_distance,
            })
        },
    )?;
    Some(TransportLaneKey::Belt {
        entity_id,
        lane_index,
    })
}

pub(in crate::simulation::belt_ops) fn splitter_output_lane_key(
    entities: &EntityStore,
    entity_id: EntityId,
    output_port: usize,
    lane_index: usize,
) -> Option<TransportLaneKey> {
    let placed = entities.placed_entities.get(&entity_id)?;
    let state = entities.splitters.get(&entity_id)?;
    let port_tile = splitter_port_tiles(placed)?.get(output_port).copied()?;
    let (dx, dy) = direction_tile_delta(state.dir);
    let endpoint = transport_endpoint_at(
        entities,
        port_tile.0 + i64::from(dx),
        port_tile.1 + i64::from(dy),
    )?;

    Some(endpoint_lane_key(endpoint, lane_index))
}

fn transport_endpoint_at(
    entities: &EntityStore,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<TransportEndpoint> {
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

fn underground_part(segment: &BeltSegment) -> Option<UndergroundBeltPart> {
    segment
        .underground
        .as_ref()
        .map(|underground| underground.part)
}
