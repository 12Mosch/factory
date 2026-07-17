use super::super::*;
use super::support::*;

#[test]
fn burner_drill_without_fuel_remains_idle() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, coal);

    for _ in 0..240 {
        sim.tick();
    }

    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        0.0
    );
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(state.output_slot.stack(), None);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
}

#[test]
fn burner_drill_with_coal_mines_output() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, coal, 1);
    crate::entity_transfer::player_slot_to_mining_drill_fuel(&mut sim, entity_id, 0)
        .expect("coal should transfer to drill fuel");

    for _ in 0..240 {
        sim.tick();
    }

    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("burner drill should expose state");
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_ore, 1)));
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        3_400_000.0
    );
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before - 1));
}

#[test]
fn one_coal_powers_burner_drill_for_exactly_1600_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, coal, 1);
    crate::entity_transfer::player_slot_to_mining_drill_fuel(&mut sim, entity_id, 0)
        .expect("coal should transfer to drill fuel");

    for _ in 0..1600 {
        sim.tick();
    }

    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .fuel_slot
            .stack(),
        None
    );
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        0.0
    );
    assert_eq!(
        state.output_slot.stack().map(|stack| stack.count()),
        Some(6)
    );
    assert_eq!(state.mining_progress_ticks, 160);

    sim.tick();

    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        0.0
    );
    assert_eq!(state.mining_progress_ticks, 160);
}

#[test]
fn blocked_burner_drill_output_pauses_without_consuming_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
    let state = sim
        .entities
        .mining_drill_state_mut(entity_id)
        .expect("burner drill should expose state");
    state.energy.burner_mut().expect("burner machine").fuel_slot = test_slot(test_stack(coal, 1));
    state.output_slot = test_slot(test_stack(coal, 1));

    for _ in 0..10 {
        sim.tick();
    }

    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .fuel_slot
            .stack(),
        Some(test_stack(coal, 1))
    );
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        0.0
    );
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
}

#[test]
fn invalid_burner_drill_fuel_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, iron_ore);
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_ore, 1);

    assert_eq!(
        crate::entity_transfer::player_slot_to_mining_drill_fuel(&mut sim, entity_id, 0),
        Err(MiningDrillError::InvalidFuel(iron_ore))
    );
    assert_eq!(
        crate::entity_access::mining_drill_state(&sim, entity_id)
            .expect("burner drill should expose state")
            .energy
            .fuel_slot()
            .expect("burner drill has a fuel slot"),
        None
    );
    assert_eq!(
        sim.player_inventory.slots()[0],
        Some(test_stack(iron_ore, 1))
    );
}

#[test]
fn burner_drill_outputs_ore_after_required_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::mining_drill_state(&sim, entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        Some(test_stack(iron_ore, 1))
    );
}

#[test]
fn burner_drill_consumes_resource_tile() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, x, y, before) = place_burner_drill_on_resource(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before - 1));
}

#[test]
fn burner_drill_outputs_ore_onto_belt() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (drill_id, belt_id, _, _, _) = place_burner_drill_outputting_to_belt(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::mining_drill_state(&sim, drill_id)
            .expect("drill should expose state")
            .output_slot,
        None
    );
    assert!(
        crate::entity_access::belt_segment(&sim, belt_id)
            .expect("belt should expose state")
            .lanes
            .iter()
            .any(|lane| lane.items.iter().any(|item| item.item_id == iron_ore))
    );
}

#[test]
fn burner_drill_exports_stored_output_onto_belt_without_new_production() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (drill_id, _, x, y, before) = place_burner_drill_outputting_to_belt(&mut sim, iron_ore);
    let state = sim
        .entities
        .mining_drill_state_mut(drill_id)
        .expect("burner drill should expose state");
    state.output_slot = test_slot(test_stack(iron_ore, 3));

    sim.tick();

    assert_eq!(
        crate::entity_access::mining_drill_state(&sim, drill_id)
            .expect("burner drill should expose state")
            .output_slot,
        Some(test_stack(iron_ore, 2))
    );
    assert_eq!(total_belt_count_for_item(&sim, iron_ore), 1);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
}

#[test]
fn burner_drill_blocks_when_output_inventory_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (drill_id, chest_id, x, y, before) =
        place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);
    fill_inventory_with(&mut sim, chest_id, coal);

    for _ in 0..240 {
        sim.tick();
    }

    let state = crate::entity_access::mining_drill_state(&sim, drill_id)
        .expect("burner drill should expose state");
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        0.0
    );
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .fuel_slot
            .stack(),
        Some(test_stack(coal, 1))
    );
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(resource_amount_at(&sim.world, x, y), Some(before));
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        0
    );
}

#[test]
fn burner_drill_outputs_into_adjacent_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (drill_id, chest_id, _, _, _) = place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1
    );
    assert_eq!(
        crate::entity_access::mining_drill_state(&sim, drill_id)
            .expect("burner drill should expose state")
            .output_slot,
        None
    );
}

#[test]
fn burner_drill_placed_on_coal_produces_coal() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);
    add_fuel_to_burner_drill(&mut sim, entity_id, coal, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::mining_drill_state(&sim, entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        Some(test_stack(coal, 1))
    );
}

#[test]
fn burner_drill_without_resource_in_mining_area_refuses_placement() {
    let sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 2, 2);

    assert!(matches!(
        crate::placement::validate(
            &sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: drill,
                x,
                y,
                direction: Direction::North
            }
        ),
        Err(BuildError::TileBlocked { .. })
    ));
}

#[test]
fn burner_drill_hash_is_deterministic_for_same_seed_and_inputs() {
    let mut a = Simulation::new_test_world(123);
    let mut b = Simulation::new_test_world(123);
    let coal = item_id(&a.world.prototypes, "coal");
    let a_entity = place_burner_drill_on_resource(&mut a, coal).0;
    let b_entity = place_burner_drill_on_resource(&mut b, coal).0;

    for (sim, entity_id) in [(&mut a, a_entity), (&mut b, b_entity)] {
        sim.player_inventory = Inventory::player();
        set_inventory_slot(&mut sim.player_inventory, 0, coal, 2);
        crate::entity_transfer::player_slot_to_mining_drill_fuel(sim, entity_id, 0)
            .expect("coal should transfer to drill fuel");
    }

    for _ in 0..1000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn electric_mining_drill_mines_only_while_powered() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (entity_id, _, _, _) =
        place_named_drill_on_resource(&mut sim, "electric_mining_drill", iron_ore);

    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("electric drill should expose state");
    assert_eq!(state.energy, MachineEnergy::Electric);
    assert_eq!(state.mining_required_ticks, 120);
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, coal, 1);
    assert_eq!(
        crate::entity_transfer::player_slot_to_mining_drill_fuel(&mut sim, entity_id, 0),
        Err(MiningDrillError::NoFuelSlot)
    );

    // Unpowered: the drill makes no progress.
    for _ in 0..120 {
        sim.advance_machines(&mut NoopTickProfiler);
    }
    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("electric drill should expose state");
    assert_eq!(state.mining_progress_ticks, 0);
    assert_eq!(state.output_slot.stack(), None);

    // Fully powered (status faked so no power plant is needed): one ore
    // every 120 ticks.
    sim.power.entity_statuses.insert(
        entity_id,
        EntityPowerStatus {
            satisfaction_permyriad: 10_000,
            ..EntityPowerStatus::default()
        },
    );
    for _ in 0..120 {
        sim.advance_machines(&mut NoopTickProfiler);
    }
    let state = crate::entity_access::mining_drill_state(&sim, entity_id)
        .expect("electric drill should expose state");
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_ore, 1)));
}
