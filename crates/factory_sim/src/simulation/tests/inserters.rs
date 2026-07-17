use super::super::*;
use super::support::*;

#[test]
fn inserter_does_not_place_invalid_items_into_lab() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (chest_id, inserter_id, lab_id) = place_chest_inserter_lab_line(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should expose inventory"),
        0,
        iron_plate,
        1,
    );

    for _ in 0..100 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should expose inventory")
            .count(iron_plate),
        1
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .expect("lab should expose inventory")
            .count(iron_plate),
        0
    );
    assert_eq!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should expose state"),
        &InserterState::WaitingForItem
    );
}

#[test]
fn inserter_moves_item_from_chest_to_furnace() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    assert!(matches!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        InserterState::WaitingForItem | InserterState::Dropping { .. }
    ));
    assert!(!matches!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        InserterState::Holding { .. }
    ));
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn inserter_moves_fuel_from_chest_to_furnace_fuel_slot() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        coal,
        1,
    );

    run_inserter_until_idle(&mut sim, inserter_id);

    let furnace =
        crate::entity_access::furnace_state(&sim, furnace_id).expect("furnace should have state");
    assert_eq!(furnace.input_slot.stack(), None);
    assert_eq!(
        furnace.energy.fuel_slot().expect("burner furnace").stack(),
        Some(test_stack(coal, 1))
    );
}

#[test]
fn inserter_waits_when_target_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let stack_size =
        item_stack_size(&sim.world.prototypes, iron_ore).expect("iron ore should have stack size");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );
    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .input_slot = test_slot(test_stack(iron_ore, stack_size));

    for _ in 0..BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 10 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        &InserterState::WaitingForItem
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, stack_size))
    );
    assert!(!matches!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        InserterState::Holding { .. }
    ));
    assert_eq!(
        total_item_count_in_sim(&sim, iron_ore),
        u32::from(stack_size) + 1
    );
}

#[test]
fn inserter_preserves_item_count() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, _inserter_id, _furnace_id) = place_chest_inserter_furnace_line(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        3,
    );

    let ticks = (BASIC_INSERTER_PICKUP_TICKS + BASIC_INSERTER_DROP_TICKS + 5) * 3;
    for _ in 0..ticks {
        sim.tick();
        assert_eq!(total_item_count_in_sim(&sim, iron_ore), 3);
    }
}

#[test]
fn inserter_moves_item_from_belt_to_furnace() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (belt_id, inserter_id, furnace_id) = place_belt_inserter_furnace_line(&mut sim);

    sim.insert_item_onto_belt(belt_id, 0, iron_ore)
        .expect("belt should accept ore");

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(total_belt_count_for_item(&sim, iron_ore), 0);
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn inserter_moves_furnace_output_to_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (furnace_id, inserter_id, chest_id) = place_furnace_inserter_chest_line(&mut sim);

    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .output_slot = test_slot(test_stack(iron_plate, 1));

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .output_slot,
        None
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        1
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_plate), 1);
}

#[test]
fn inserter_moves_furnace_output_to_belt() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (furnace_id, inserter_id, _belt_id) = place_furnace_inserter_belt_line(&mut sim);

    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .output_slot = test_slot(test_stack(iron_plate, 1));

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .output_slot,
        None
    );
    assert_eq!(total_belt_count_for_item(&sim, iron_plate), 1);
    assert_eq!(total_item_count_in_sim(&sim, iron_plate), 1);
}

#[test]
fn inserter_uses_rotated_direction_for_pickup_and_drop() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (x, y) = place_powered_fixture_origin(&mut sim, 4, 2, (1, 2));

    let chest_id = place_at(&mut sim, chest, x, y, Direction::North);
    let inserter_id = place_at(&mut sim, inserter, x + 1, y, Direction::North);
    let furnace_id = place_at(&mut sim, furnace, x + 2, y, Direction::North);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );

    for _ in 0..BASIC_INSERTER_PICKUP_TICKS + 2 {
        sim.tick();
    }
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        None
    );

    crate::entity_mutation::rotate(&mut sim, inserter_id, Direction::East)
        .expect("inserter should rotate");
    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn fast_inserter_transfers_faster_than_basic() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (x, y) = place_powered_fixture_origin(&mut sim, 4, 5, (1, 2));

    let (basic_source, _basic_inserter, basic_target) =
        place_chest_inserter_furnace_line_at(&mut sim, "inserter", x, y);
    let (fast_source, _fast_inserter, fast_target) =
        place_chest_inserter_furnace_line_at(&mut sim, "fast_inserter", x, y + 3);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, basic_source)
            .expect("basic source chest should have inventory"),
        0,
        iron_ore,
        1,
    );
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, fast_source)
            .expect("fast source chest should have inventory"),
        0,
        iron_ore,
        1,
    );

    for _ in 0..20 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::furnace_state(&sim, fast_target)
            .expect("fast target should be a furnace")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, basic_target)
            .expect("basic target should be a furnace")
            .input_slot,
        None
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, basic_source)
            .expect("basic source chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 2);
}

#[test]
fn long_handed_inserter_uses_two_tile_pickup_and_drop() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, inserter_id, furnace_id) =
        place_two_tile_chest_inserter_furnace_line(&mut sim, "long_handed_inserter");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}

#[test]
fn basic_inserter_does_not_reach_long_handed_positions() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (chest_id, inserter_id, furnace_id) =
        place_two_tile_chest_inserter_furnace_line(&mut sim, "inserter");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );

    for _ in 0..inserter_cycle_tick_budget(&sim, inserter_id) {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        None
    );
    assert_eq!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        &InserterState::WaitingForItem
    );
}

#[test]
fn inserter_holding_item_does_not_duplicate_when_target_becomes_blocked() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let stack_size = item_stack_size(&sim.world.prototypes, copper_ore)
        .expect("copper ore should have stack size");
    let (chest_id, inserter_id, furnace_id) = place_chest_inserter_furnace_line(&mut sim);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );

    run_inserter_until_holding(&mut sim, inserter_id);
    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .input_slot = test_slot(test_stack(copper_ore, stack_size));

    for _ in 0..inserter_cycle_tick_budget(&sim, inserter_id) * 3 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(copper_ore, stack_size))
    );
    assert_eq!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        &InserterState::Holding {
            item: test_stack(iron_ore, 1),
        }
    );
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
    assert_eq!(
        total_item_count_in_sim(&sim, copper_ore),
        u32::from(stack_size)
    );

    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should have state")
        .input_slot = ItemSlot::default();
    sim.tick();

    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    assert!(!matches!(
        crate::entity_access::inserter_state(&sim, inserter_id)
            .expect("inserter should have state"),
        InserterState::Holding { .. }
    ));
    assert_eq!(total_item_count_in_sim(&sim, iron_ore), 1);
}
