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
            .recipes
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
