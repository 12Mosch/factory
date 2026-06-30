use super::super::*;
use super::support::*;

#[test]
fn catalog_loads_assembler_metadata() {
    let sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let prototype = &sim.world.prototypes.entities[assembler.index()];
    let metadata = prototype
        .assembling_machine
        .as_ref()
        .expect("assembler prototype should load metadata");

    assert_eq!(prototype.entity_kind, EntityKind::AssemblingMachine);
    assert_eq!((prototype.size.x, prototype.size.y), (3, 3));
    assert_eq!(metadata.crafting_speed_numerator, 1);
    assert_eq!(metadata.crafting_speed_denominator, 2);
    assert_eq!(
        metadata.input_slot_count,
        ASSEMBLING_MACHINE_INPUT_SLOT_COUNT
    );
    assert_eq!(
        metadata.output_slot_count,
        ASSEMBLING_MACHINE_OUTPUT_SLOT_COUNT
    );
}

#[test]
fn assembler_crafts_gears_from_iron_plates() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 0);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(state.crafting_required_ticks, 60);
}

#[test]
fn assembler_blocks_without_inputs() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept partial ingredients");

    for _ in 0..90 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 1);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn assembler_blocks_when_output_full() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let stack_size = item_stack_size(&sim.world.prototypes, iron_gear_wheel)
        .expect("gear should have stack size");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    sim.transfer_player_slot_to_assembler_input(assembler_id, 0)
        .expect("assembler should accept gear ingredients");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .output_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_gear_wheel,
        count: stack_size,
    });

    for _ in 0..60 {
        sim.tick();
    }

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 2);
    assert_eq!(
        state.output_inventory.count(iron_gear_wheel),
        u32::from(stack_size)
    );
    assert_eq!(state.crafting_progress_ticks, 0);
}

#[test]
fn invalid_assembler_recipe_is_rejected() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let smelting_recipe = recipe_id(&sim.world.prototypes, "iron_plate");

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, smelting_recipe),
        Err(AssemblerError::InvalidRecipe(smelting_recipe))
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state")
            .selected_recipe,
        None
    );
}

#[test]
fn selecting_different_assembler_recipe_on_empty_assembler_succeeds() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.select_assembler_recipe(assembler_id, cable_recipe)
        .expect("empty assembler should allow recipe changes");

    let state = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.selected_recipe, Some(cable_recipe));
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(state.crafting_required_ticks, 60);
}

#[test]
fn selecting_same_assembler_recipe_while_non_empty_preserves_progress() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    {
        let state = sim
            .entities
            .assembler_state_mut(assembler_id)
            .expect("assembler should expose mutable state");
        state.input_inventory.slots[0] = Some(ItemStack {
            item_id: iron_plate,
            count: 1,
        });
        state.crafting_progress_ticks = 17;
    }
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("same recipe selection should be idempotent");

    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn selecting_different_assembler_recipe_with_input_items_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .input_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, cable_recipe),
        Err(AssemblerError::RecipeChangeRequiresEmpty {
            entity_id: assembler_id
        })
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn selecting_different_assembler_recipe_with_output_items_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .output_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_gear_wheel,
        count: 1,
    });
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, cable_recipe),
        Err(AssemblerError::RecipeChangeRequiresEmpty {
            entity_id: assembler_id
        })
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn selecting_different_assembler_recipe_with_progress_fails_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");

    sim.select_assembler_recipe(assembler_id, gear_recipe)
        .expect("initial recipe should be accepted");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .crafting_progress_ticks = 1;
    let before = sim
        .assembler_state(assembler_id)
        .expect("assembler should expose state")
        .clone();

    assert_eq!(
        sim.select_assembler_recipe(assembler_id, cable_recipe),
        Err(AssemblerError::RecipeChangeRequiresEmpty {
            entity_id: assembler_id
        })
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state"),
        &before
    );
}

#[test]
fn assembler_ingredient_status_reports_partial_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .input_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    assert_eq!(
        sim.assembler_ingredient_status(assembler_id)
            .expect("ingredient status should be available"),
        vec![AssemblerIngredientStatus {
            item: iron_plate,
            required: 2,
            available: 1,
            missing: 1,
        }]
    );
}

#[test]
fn inserter_moves_ingredients_from_chest_to_assembler() {
    let mut sim = Simulation::new_test_world(123);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let (chest_id, inserter_id, assembler_id) = place_chest_inserter_assembler_line(&mut sim);
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.entity_inventory_mut(chest_id)
        .expect("chest should have inventory")
        .slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        0
    );
    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state")
            .input_inventory
            .count(iron_plate),
        1
    );
}

#[test]
fn inserter_removes_assembler_output_to_chest() {
    let mut sim = Simulation::new_test_world(123);
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let (assembler_id, inserter_id, chest_id) = place_assembler_inserter_chest_line(&mut sim);
    sim.entities
        .assembler_state_mut(assembler_id)
        .expect("assembler should expose mutable state")
        .output_inventory
        .slots[0] = Some(ItemStack {
        item_id: iron_gear_wheel,
        count: 1,
    });

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        sim.assembler_state(assembler_id)
            .expect("assembler should expose state")
            .output_inventory
            .count(iron_gear_wheel),
        0
    );
    assert_eq!(
        sim.entity_inventory(chest_id)
            .expect("chest should have inventory")
            .count(iron_gear_wheel),
        1
    );
}

#[test]
fn assembler_state_hash_remains_deterministic_for_same_seed_actions() {
    let mut first = Simulation::new_test_world(123);
    let mut second = Simulation::new_test_world(123);
    run_same_assembler_actions(&mut first);
    run_same_assembler_actions(&mut second);

    assert_eq!(first.state_hash(), second.state_hash());
}
