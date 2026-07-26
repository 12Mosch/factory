use super::super::super::*;
use super::*;

/// Places one roboport on the first buildable spot and settles the topology so
/// its logistic coverage can be asked about.
pub(in crate::simulation::tests) fn place_roboport(
    sim: &mut Simulation,
) -> (EntityId, (WorldTileCoord, WorldTileCoord)) {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, "roboport");
    let (x, y) = first_placeable_entity_tile(sim, prototype_id, Direction::North);
    let roboport = place_at(sim, prototype_id, x, y, Direction::North);
    sim.tick();
    (roboport, (x, y))
}

/// Stations `count` robots of the named item in a roboport and fills its
/// charging buffer.
///
/// Dispatch pays for a robot's full charge out of the buffer, so a fixture that
/// only stationed robots could never send one; filling the buffer directly
/// stands in for the power fixture the robot rules do not need.
pub(in crate::simulation::tests) fn station_robots(
    sim: &mut Simulation,
    roboport: EntityId,
    robot_name: &str,
    count: u16,
) {
    let catalog = sim.world.prototypes.clone();
    let robot_item = item_id(&catalog, robot_name);
    let capacity = sim
        .entity_roboport_status(roboport)
        .expect("a placed roboport reports status")
        .charge_capacity_joules;

    let state = sim
        .entities
        .roboport_state_mut(roboport)
        .expect("roboport state exists");
    state
        .robots
        .insert(&catalog, robot_item, count)
        .expect("the robot slots should hold the fixture's robots");
    state.charge_energy_joules = capacity;
    sim.robots.mark_roboport_dirty(roboport);
    sim.tick();
}

/// Places a chest on the covered, buildable tile nearest `near`.
///
/// Nearest rather than first, so the chest also lands inside circuit wire reach
/// of the roboport; a chest 25 tiles away is still covered but could not be
/// wired to it.
pub(in crate::simulation::tests) fn place_covered_chest(
    sim: &mut Simulation,
    chest_name: &str,
    near: (WorldTileCoord, WorldTileCoord),
) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, chest_name);
    let (x, y) = all_tile_coords(&sim.world)
        .into_iter()
        .filter(|(x, y)| {
            sim.logistic_network_covering_tile(*x, *y).is_some()
                && crate::placement::validate(
                    sim,
                    crate::placement::EntityPlacementRequest {
                        prototype_id,
                        x: *x,
                        y: *y,
                        direction: Direction::North,
                    },
                )
                .is_ok()
        })
        .min_by_key(|(x, y)| (x - near.0).pow(2) + (y - near.1).pow(2))
        .expect("a roboport's logistic square should contain a buildable tile");
    place_at(sim, prototype_id, x, y, Direction::North)
}

/// Places a chest on the first buildable tile no roboport reaches.
pub(in crate::simulation::tests) fn place_uncovered_chest(
    sim: &mut Simulation,
    chest_name: &str,
) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, chest_name);
    let (x, y) = all_tile_coords(&sim.world)
        .into_iter()
        .find(|(x, y)| {
            sim.logistic_network_covering_tile(*x, *y).is_none()
                && crate::placement::validate(
                    sim,
                    crate::placement::EntityPlacementRequest {
                        prototype_id,
                        x: *x,
                        y: *y,
                        direction: Direction::North,
                    },
                )
                .is_ok()
        })
        .expect("the generated world is larger than one roboport's reach");
    place_at(sim, prototype_id, x, y, Direction::North)
}

pub(in crate::simulation::tests) fn insert_into_chest(
    sim: &mut Simulation,
    chest: EntityId,
    item_id: ItemId,
    count: u16,
) {
    let catalog = sim.world.prototypes.clone();
    crate::entity_access::inventory_mut(sim, chest)
        .expect("a chest has an inventory")
        .insert(&catalog, item_id, count)
        .expect("the chest should accept the item");
}

pub(in crate::simulation::tests) fn chest_count(
    sim: &Simulation,
    chest: EntityId,
    item_id: ItemId,
) -> u32 {
    sim.entities
        .entity_inventories
        .get(&chest)
        .map_or(0, |inventory| inventory.count(item_id))
}

/// Asks a requester or buffer chest for `count` of `item_id` on its first row.
pub(in crate::simulation::tests) fn request_items(
    sim: &mut Simulation,
    chest: EntityId,
    item_id: ItemId,
    count: u32,
) {
    sim.set_logistic_request(
        chest,
        0,
        LogisticRequest {
            item: Some(item_id),
            count,
        },
    )
    .expect("a requesting chest takes an amount");
}
