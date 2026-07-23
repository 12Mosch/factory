use super::*;

#[derive(Clone, Copy)]
pub(super) struct UndergroundEndpoint {
    pub(super) part: UndergroundBeltPart,
    pub(super) max_distance: u8,
}

/// Finds the complementary underground endpoint on the placed entity's axis.
/// The surface endpoints may have different state types (belts and pipes), so
/// the caller supplies the metadata lookup while distance, orientation, and
/// entrance/exit rules remain shared.
pub(super) fn paired_underground_entity(
    entities: &EntityStore,
    placed: &PlacedEntity,
    endpoint: UndergroundEndpoint,
    endpoint_for: impl Fn(EntityId) -> Option<UndergroundEndpoint>,
) -> Option<EntityId> {
    let (mut dx, mut dy) = direction_tile_delta(placed.direction);
    let expected_part = match endpoint.part {
        UndergroundBeltPart::Entrance => UndergroundBeltPart::Exit,
        UndergroundBeltPart::Exit => {
            dx = -dx;
            dy = -dy;
            UndergroundBeltPart::Entrance
        }
    };
    let max_offset = i32::from(endpoint.max_distance) + 1;

    for offset in 1..=max_offset {
        let Some(candidate_id) = entities.occupancy.entity_at(
            placed.x + i64::from(dx * offset),
            placed.y + i64::from(dy * offset),
        ) else {
            continue;
        };
        let Some(candidate) = entities.placed_entity(candidate_id) else {
            continue;
        };
        let Some(candidate_endpoint) = endpoint_for(candidate_id) else {
            continue;
        };
        let underground_distance = (offset - 1) as u8;
        if candidate_endpoint.part == expected_part
            && candidate.direction == placed.direction
            && underground_distance <= endpoint.max_distance
            && underground_distance <= candidate_endpoint.max_distance
        {
            return Some(candidate_id);
        }
    }

    None
}

pub(super) fn direction_tile_delta(direction: Direction) -> (i32, i32) {
    match direction {
        Direction::North => (0, 1),
        Direction::East => (1, 0),
        Direction::South => (0, -1),
        Direction::West => (-1, 0),
    }
}
