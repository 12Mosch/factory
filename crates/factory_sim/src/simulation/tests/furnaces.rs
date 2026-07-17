use super::super::*;
use super::support::*;

#[test]
fn furnace_smelts_iron_ore_to_iron_plate() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.input_slot.stack(), None);
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_plate, 1)));
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        3_685_000.0
    );
}

#[test]
fn furnace_does_not_smelts_without_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let entity_id = place_stone_furnace(&mut sim);
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_ore, 1);
    crate::entity_transfer::player_slot_to_furnace_input(&mut sim, entity_id, 0)
        .expect("ore should transfer to furnace input");

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.input_slot.stack(), Some(test_stack(iron_ore, 1)));
    assert_eq!(state.output_slot.stack(), None);
    assert_eq!(
        state
            .energy
            .burner()
            .expect("burner machine")
            .energy_remaining_joules,
        0.0
    );
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn furnace_blocks_when_output_full() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);
    let state = sim
        .entities
        .furnace_state_mut(entity_id)
        .expect("furnace should expose state");
    state.output_slot = test_slot(test_stack(copper_plate, 1));

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.input_slot.stack(), Some(test_stack(iron_ore, 1)));
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
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        state.output_slot.stack().map(|stack| stack.item_id()),
        Some(copper_plate)
    );
    assert_eq!(
        state
            .output_slot
            .stack()
            .map(|stack| stack.item_id() == iron_plate),
        Some(false)
    );
}

#[test]
fn furnace_smelts_copper_ore_to_copper_plate() {
    let mut sim = Simulation::new_test_world(123);
    let copper_ore = item_id(&sim.world.prototypes, "copper_ore");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, copper_ore, coal);

    for _ in 0..210 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::furnace_state(&sim, entity_id)
            .expect("furnace should expose state")
            .output_slot,
        Some(test_stack(copper_plate, 1))
    );
}

#[test]
fn furnace_smelts_stone_to_stone_brick() {
    let mut sim = Simulation::new_test_world(123);
    let stone = item_id(&sim.world.prototypes, "stone");
    let stone_brick = item_id(&sim.world.prototypes, "stone_brick");
    let coal = item_id(&sim.world.prototypes, "coal");
    let recipe = recipe_id(&sim.world.prototypes, "stone_brick");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, stone, coal);

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.active_recipe, Some(recipe));
    assert_eq!(state.output_slot.stack(), Some(test_stack(stone_brick, 1)));
}

#[test]
fn invalid_furnace_input_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_stone_furnace(&mut sim);
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, coal, 1);

    assert_eq!(
        crate::entity_transfer::player_slot_to_furnace_input(&mut sim, entity_id, 0),
        Err(FurnaceError::InvalidInput(coal))
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, entity_id)
            .expect("furnace should expose state")
            .input_slot,
        None
    );
    assert_eq!(sim.player_inventory.slots()[0], Some(test_stack(coal, 1)));
}

#[test]
fn locked_smelting_recipes_are_not_selected_by_furnaces() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let stone_brick = recipe_id(&catalog, "stone_brick");
    catalog.technologies[0]
        .effects
        .push(TechnologyEffect::UnlockRecipe(stone_brick));
    let mut sim = Simulation::new(123, catalog);
    let furnace_id = place_stone_furnace(&mut sim);
    let stone = item_id(&sim.world.prototypes, "stone");
    sim.entities
        .furnace_state_mut(furnace_id)
        .expect("furnace should expose state")
        .input_slot = test_slot(test_stack(stone, 1));

    for _ in 0..240 {
        sim.tick();
    }

    let furnace =
        crate::entity_access::furnace_state(&sim, furnace_id).expect("furnace should expose state");
    assert_eq!(furnace.active_recipe, None);
    assert_eq!(furnace.input_slot.stack().unwrap().count(), 1);
    assert_eq!(
        sim.technology_progress(technology_id(&sim.world.prototypes, "automation")),
        Some(0)
    );
}

#[test]
fn steel_furnace_smelts_at_double_speed() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let entity_id = place_named_furnace(&mut sim, "steel_furnace");
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, coal);

    // A stone furnace needs 210 ticks for iron plates; the steel furnace's
    // 2x crafting speed halves that.
    for _ in 0..105 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.input_slot.stack(), None);
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_plate, 1)));
}

#[test]
fn electric_furnace_smelts_from_grid_power_without_fuel_slot() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let furnace = entity_id_by_name(&sim.world.prototypes, "electric_furnace");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: furnace,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("electric furnace should be placeable");

    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_ore, 1);
    crate::entity_transfer::player_slot_to_furnace_input(&mut sim, entity_id, 0)
        .expect("ore should transfer to electric furnace input");
    set_inventory_slot(&mut sim.player_inventory, 1, coal, 1);
    assert_eq!(
        crate::entity_transfer::player_slot_to_furnace_fuel(&mut sim, entity_id, 1),
        Err(FurnaceError::NoFuelSlot)
    );

    // 2x crafting speed halves the 210-tick iron plate recipe; extra ticks
    // cover the boiler and steam engine spinning up.
    for _ in 0..160 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.energy, MachineEnergy::Electric);
    assert_eq!(state.input_slot.stack(), None);
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_plate, 1)));
}

#[test]
fn electric_furnace_without_power_reports_no_power() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let entity_id = place_named_furnace(&mut sim, "electric_furnace");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_ore, 1);
    crate::entity_transfer::player_slot_to_furnace_input(&mut sim, entity_id, 0)
        .expect("ore should transfer to electric furnace input");

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.input_slot.stack(), Some(test_stack(iron_ore, 1)));
    assert_eq!(state.output_slot.stack(), None);
    assert_eq!(
        sim.machine_status_for_entity(entity_id),
        Some(MachineStatus::NoPower)
    );
}
