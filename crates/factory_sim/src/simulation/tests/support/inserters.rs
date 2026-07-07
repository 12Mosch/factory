use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn place_unpowered_chest_inserter_furnace_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
    place_chest_inserter_furnace_line_at(sim, "inserter", x, y)
}

pub(in crate::simulation::tests) fn place_chest_inserter_furnace_line_at(
    sim: &mut Simulation,
    inserter_name: &str,
    x: i32,
    y: i32,
) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, inserter_name);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let chest_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y, Direction::East)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 2, y, Direction::North)
        .expect("furnace should be placeable");

    (chest_id, inserter_id, furnace_id)
}

pub(in crate::simulation::tests) fn place_chest_inserter_furnace_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (1, 2));
    place_chest_inserter_furnace_line_at(sim, "inserter", x, y)
}

pub(in crate::simulation::tests) fn place_two_tile_chest_inserter_furnace_line(
    sim: &mut Simulation,
    inserter_name: &str,
) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, inserter_name);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = place_powered_fixture_origin(sim, 6, 2, (2, 2));
    let chest_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 2, y, Direction::East)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 4, y, Direction::North)
        .expect("furnace should be placeable");

    (chest_id, inserter_id, furnace_id)
}

pub(in crate::simulation::tests) fn place_chest_inserter_assembler_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(sim, 5, 3, (1, 3));
    let chest_id = sim
        .place_entity(chest, x, y + 1, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y + 1, Direction::East)
        .expect("inserter should be placeable");
    let assembler_id = sim
        .place_entity(assembler, x + 2, y, Direction::North)
        .expect("assembler should be placeable");

    (chest_id, inserter_id, assembler_id)
}

pub(in crate::simulation::tests) fn place_chest_inserter_lab_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let (x, y) = place_powered_fixture_origin(sim, 5, 3, (1, 3));
    let chest_id = sim
        .place_entity(chest, x, y + 1, Direction::North)
        .expect("chest should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y + 1, Direction::East)
        .expect("inserter should be placeable");
    let lab_id = sim
        .place_entity(lab, x + 2, y, Direction::North)
        .expect("lab should be placeable");

    (chest_id, inserter_id, lab_id)
}

pub(in crate::simulation::tests) fn place_belt_inserter_furnace_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (1, 2));
    let belt_id = sim
        .place_entity(belt, x, y, Direction::East)
        .expect("belt should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 1, y, Direction::East)
        .expect("inserter should be placeable");
    let furnace_id = sim
        .place_entity(furnace, x + 2, y, Direction::North)
        .expect("furnace should be placeable");

    (belt_id, inserter_id, furnace_id)
}

pub(in crate::simulation::tests) fn place_furnace_inserter_chest_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (2, 2));
    let furnace_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 2, y, Direction::East)
        .expect("inserter should be placeable");
    let chest_id = sim
        .place_entity(chest, x + 3, y, Direction::North)
        .expect("chest should be placeable");

    (furnace_id, inserter_id, chest_id)
}

pub(in crate::simulation::tests) fn place_assembler_inserter_chest_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = place_powered_fixture_origin(sim, 5, 3, (1, 3));
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 3, y + 1, Direction::East)
        .expect("inserter should be placeable");
    let chest_id = sim
        .place_entity(chest, x + 4, y + 1, Direction::North)
        .expect("chest should be placeable");

    (assembler_id, inserter_id, chest_id)
}

pub(in crate::simulation::tests) fn place_furnace_inserter_belt_line(
    sim: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let (x, y) = place_powered_fixture_origin(sim, 4, 2, (2, 2));
    let furnace_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    let inserter_id = sim
        .place_entity(inserter, x + 2, y, Direction::East)
        .expect("inserter should be placeable");
    let belt_id = sim
        .place_entity(belt, x + 3, y, Direction::East)
        .expect("belt should be placeable");

    (furnace_id, inserter_id, belt_id)
}

pub(in crate::simulation::tests) fn run_inserter_until_idle(
    sim: &mut Simulation,
    inserter_id: EntityId,
) {
    for _ in 0..inserter_cycle_tick_budget(sim, inserter_id) {
        sim.tick();
        if matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::WaitingForItem
        ) {
            return;
        }
    }

    panic!("inserter did not return to idle");
}

pub(in crate::simulation::tests) fn run_inserter_until_holding(
    sim: &mut Simulation,
    inserter_id: EntityId,
) {
    for _ in 0..inserter_cycle_tick_budget(sim, inserter_id) {
        sim.tick();
        if matches!(
            sim.inserter_state(inserter_id)
                .expect("inserter should have state"),
            InserterState::Holding { .. }
        ) {
            return;
        }
    }

    panic!("inserter did not pick up an item");
}

pub(in crate::simulation::tests) fn inserter_cycle_tick_budget(
    sim: &Simulation,
    inserter_id: EntityId,
) -> u32 {
    let placed = sim
        .entities
        .placed_entity(inserter_id)
        .expect("inserter should be placed");
    let prototype = sim
        .world
        .prototypes
        .entity(placed.prototype_id)
        .expect("inserter prototype should exist");
    let inserter = prototype
        .inserter
        .as_ref()
        .expect("inserter prototype should define metadata");

    inserter.pickup_ticks + inserter.drop_ticks + 20
}
