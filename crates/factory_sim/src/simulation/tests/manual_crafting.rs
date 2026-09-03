use super::super::*;

#[test]
fn crafting_consumes_ingredients_and_outputs_product() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .expect("test inventory should accept ingredients");

    sim.start_manual_craft(recipe)
        .expect("craft should start with enough ingredients");

    assert_eq!(sim.player_inventory.count(iron_plate), 0);
    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
    assert_eq!(
        sim.crafting_queue.entries.front(),
        Some(&CraftingJob {
            id: CraftingJobId(0),
            recipe_id: recipe,
            remaining_ticks: 30,
        })
    );

    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn crafting_does_not_start_without_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 1)
        .expect("test inventory should accept partial ingredients");
    let before = sim.player_inventory.clone();

    assert_eq!(
        sim.start_manual_craft(recipe),
        Err(CraftingError::InsufficientIngredients)
    );
    assert_eq!(sim.player_inventory, before);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn crafting_product_appears_only_after_configured_ticks() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "transport_belt");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let transport_belt = item_id(&sim.world.prototypes, "transport_belt");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 1)
        .expect("test inventory should accept iron plate");
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_gear_wheel, 1)
        .expect("test inventory should accept gear");

    sim.start_manual_craft(recipe)
        .expect("craft should start with enough ingredients");
    for _ in 0..29 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(transport_belt), 0);
    assert_eq!(
        sim.crafting_queue
            .entries
            .front()
            .map(|job| job.remaining_ticks),
        Some(1)
    );

    sim.tick();

    assert_eq!(sim.player_inventory.count(transport_belt), 2);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn full_inventory_pauses_completed_craft_until_space_is_freed() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let coal = item_id(&sim.world.prototypes, "coal");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .expect("single stack should fit ingredients");
    sim.start_manual_craft(recipe)
        .expect("craft should start with enough ingredients");
    sim.player_inventory
        .insert(&sim.world.prototypes, coal, 100)
        .expect("blocking stack should fill inventory");

    for _ in 0..30 {
        sim.tick();
    }

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
    assert_eq!(sim.crafting_queue.entries.len(), 1);
    assert_eq!(
        sim.crafting_queue
            .entries
            .front()
            .map(|job| job.remaining_ticks),
        Some(0)
    );

    sim.tick();
    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 0);
    assert_eq!(sim.crafting_queue.entries.len(), 1);

    sim.player_inventory
        .remove(coal, 100)
        .expect("test should be able to free blocking stack");
    sim.tick();

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn cancelling_one_of_multiple_jobs_refunds_only_its_exact_reservation() {
    let mut sim = Simulation::new_test_world(123);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let pipe_recipe = recipe_id(&sim.world.prototypes, "pipe");
    let cable_recipe = recipe_id(&sim.world.prototypes, "copper_cable");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let copper_plate = item_id(&sim.world.prototypes, "copper_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 3)
        .expect("test inventory should accept iron");
    sim.player_inventory
        .insert(&sim.world.prototypes, copper_plate, 1)
        .expect("test inventory should accept copper");

    sim.start_manual_craft(gear_recipe).unwrap();
    sim.start_manual_craft(pipe_recipe).unwrap();
    sim.start_manual_craft(cable_recipe).unwrap();
    sim.cancel_manual_craft(CraftingJobId(1)).unwrap();

    assert_eq!(sim.player_inventory.count(iron_plate), 1);
    assert_eq!(sim.player_inventory.count(copper_plate), 0);
    assert_eq!(
        sim.crafting_queue
            .entries
            .iter()
            .map(|job| job.id)
            .collect::<Vec<_>>(),
        vec![CraftingJobId(0), CraftingJobId(2)]
    );
}

#[test]
fn cancelling_a_partial_active_job_refunds_every_ingredient() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let gear = item_id(&sim.world.prototypes, "iron_gear_wheel");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .unwrap();
    sim.start_manual_craft(recipe).unwrap();
    for _ in 0..10 {
        sim.tick();
    }

    sim.cancel_manual_craft(CraftingJobId(0)).unwrap();

    assert_eq!(sim.player_inventory.count(iron_plate), 2);
    assert_eq!(sim.player_inventory.count(gear), 0);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn cancellation_with_a_full_inventory_fails_without_any_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .unwrap();
    sim.start_manual_craft(recipe).unwrap();
    sim.player_inventory
        .insert(&sim.world.prototypes, coal, 100)
        .unwrap();
    let inventory_before = sim.player_inventory.clone();
    let queue_before = sim.crafting_queue.clone();
    let statistics_before = sim.item_statistics();

    assert_eq!(
        sim.cancel_manual_craft(CraftingJobId(0)),
        Err(CraftingError::RefundInventoryFull)
    );
    assert_eq!(sim.player_inventory, inventory_before);
    assert_eq!(sim.crafting_queue, queue_before);
    assert_eq!(sim.item_statistics(), statistics_before);
}

#[test]
fn reordering_jobs_preserves_partial_progress_and_reserved_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let pipe_recipe = recipe_id(&sim.world.prototypes, "pipe");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let gear = item_id(&sim.world.prototypes, "iron_gear_wheel");
    let pipe = item_id(&sim.world.prototypes, "pipe");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 3)
        .unwrap();
    sim.start_manual_craft(gear_recipe).unwrap();
    sim.start_manual_craft(pipe_recipe).unwrap();
    for _ in 0..10 {
        sim.tick();
    }

    sim.move_manual_craft(CraftingJobId(0), CraftingQueueMove::Later)
        .unwrap();

    assert_eq!(sim.player_inventory.count(iron_plate), 0);
    assert_eq!(sim.crafting_queue.entries[0].id, CraftingJobId(1));
    assert_eq!(sim.crafting_queue.entries[1].remaining_ticks, 20);
    for _ in 0..30 {
        sim.tick();
    }
    assert_eq!(sim.player_inventory.count(pipe), 1);
    assert_eq!(sim.player_inventory.count(gear), 0);
    for _ in 0..20 {
        sim.tick();
    }
    assert_eq!(sim.player_inventory.count(gear), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn stale_job_ids_cannot_mutate_a_replacement_job() {
    let mut sim = Simulation::new_test_world(123);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let pipe_recipe = recipe_id(&sim.world.prototypes, "pipe");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 3)
        .unwrap();
    sim.start_manual_craft(gear_recipe).unwrap();
    sim.cancel_manual_craft(CraftingJobId(0)).unwrap();
    sim.start_manual_craft(pipe_recipe).unwrap();
    let inventory_before = sim.player_inventory.clone();
    let queue_before = sim.crafting_queue.clone();

    assert_eq!(
        sim.cancel_manual_craft(CraftingJobId(0)),
        Err(CraftingError::MissingJob(CraftingJobId(0)))
    );
    assert_eq!(
        sim.move_manual_craft(CraftingJobId(0), CraftingQueueMove::Later),
        Err(CraftingError::MissingJob(CraftingJobId(0)))
    );
    assert_eq!(sim.player_inventory, inventory_before);
    assert_eq!(sim.crafting_queue, queue_before);
    assert_eq!(sim.crafting_queue.entries[0].id, CraftingJobId(1));
}

#[test]
fn mutated_queue_round_trips_and_continues_deterministically() {
    let mut sim = Simulation::new_test_world(123);
    let gear_recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let pipe_recipe = recipe_id(&sim.world.prototypes, "pipe");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 3)
        .unwrap();
    sim.start_manual_craft(gear_recipe).unwrap();
    sim.start_manual_craft(pipe_recipe).unwrap();
    for _ in 0..7 {
        sim.tick();
    }
    sim.move_manual_craft(CraftingJobId(0), CraftingQueueMove::Later)
        .unwrap();

    let bytes = crate::save_to_bytes(&sim).expect("mutated crafting queue should save");
    let mut loaded = crate::load_from_bytes(&bytes).expect("mutated crafting queue should load");
    assert_eq!(loaded.crafting_queue, sim.crafting_queue);
    assert_eq!(loaded.state_hash(), sim.state_hash());

    let command = SimCommand::CancelManualCraft {
        job_id: CraftingJobId(1),
    };
    sim.apply_command(&command).unwrap();
    loaded.apply_command(&command).unwrap();
    sim.apply_command(&SimCommand::StartManualCraft(pipe_recipe))
        .unwrap();
    loaded
        .apply_command(&SimCommand::StartManualCraft(pipe_recipe))
        .unwrap();
    assert_eq!(sim.crafting_queue.entries[1].id, CraftingJobId(2));
    assert_eq!(loaded.crafting_queue.entries[1].id, CraftingJobId(2));
    for _ in 0..23 {
        sim.tick();
        loaded.tick();
    }
    assert_eq!(loaded.state_hash(), sim.state_hash());
}

#[test]
fn load_rejects_manual_crafting_progress_above_recipe_duration() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .unwrap();
    sim.start_manual_craft(recipe).unwrap();
    sim.crafting_queue.entries[0].remaining_ticks = 31;

    let error = crate::load_from_bytes(&crate::save_to_bytes(&sim).unwrap())
        .expect_err("progress above the recipe duration must be rejected");

    assert!(matches!(
        error,
        SaveLoadError::InvalidSimulationState(SimulationValidationError::InvalidCraftingProgress {
            job_id: CraftingJobId(0),
            remaining_ticks: 31,
            required_ticks: 30,
        })
    ));
}

#[test]
fn statistics_count_only_successfully_completed_manual_crafts() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let gear = item_id(&sim.world.prototypes, "iron_gear_wheel");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .unwrap();

    sim.start_manual_craft(recipe).unwrap();
    assert!(sim.item_statistics().rows.is_empty());
    sim.cancel_manual_craft(CraftingJobId(0)).unwrap();
    assert!(sim.item_statistics().rows.is_empty());

    sim.start_manual_craft(recipe).unwrap();
    for _ in 0..30 {
        sim.tick();
    }
    let statistics = sim.item_statistics();
    let iron_row = statistics
        .rows
        .iter()
        .find(|row| row.item_id == iron_plate)
        .unwrap();
    let gear_row = statistics
        .rows
        .iter()
        .find(|row| row.item_id == gear)
        .unwrap();
    assert_eq!(iron_row.consumed_total, 2);
    assert_eq!(iron_row.produced_total, 0);
    assert_eq!(gear_row.produced_total, 1);
    assert_eq!(gear_row.consumed_total, 0);
}

#[test]
fn smelting_recipes_cannot_be_manually_crafted() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_plate");
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_ore, 1)
        .expect("test inventory should accept ore");

    assert_eq!(
        sim.start_manual_craft(recipe),
        Err(CraftingError::NotManualRecipe(recipe))
    );
    assert_eq!(sim.player_inventory.count(iron_ore), 1);
    assert!(sim.crafting_queue.entries.is_empty());
}

#[test]
fn base_catalog_contains_expected_manually_craftable_recipes() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let recipe_names = [
        "stone_furnace",
        "burner_mining_drill",
        "transport_belt",
        "inserter",
        "chest",
        "offshore_pump",
        "boiler",
        "steam_engine",
        "pipe",
        "small_electric_pole",
        "assembling_machine",
        "lab",
        "automation_science_pack",
    ];

    for recipe_name in recipe_names {
        let recipe = catalog
            .recipes()
            .iter()
            .find(|recipe| recipe.name == recipe_name)
            .unwrap_or_else(|| panic!("missing recipe {recipe_name:?}"));
        assert!(
            matches!(
                recipe.category,
                CraftingCategory::Crafting | CraftingCategory::Manual
            ),
            "{recipe_name} should be manually craftable"
        );
    }
}
