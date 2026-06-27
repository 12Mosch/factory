use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::{
    DebugBuildDirection, FactoryAppPlugin, InventoryPanel, SimResource,
    available_crafting_recipe_choices, crafting_recipe_choices, format_assembler_detail_text,
    handle_debug_belt_item_insertion_at_tile, handle_debug_build_action_at_tile,
    opened_container_after_world_click, transfer_open_container_slot, world_position_to_tile_coord,
};
use factory_data::{CraftingCategory, EntityKind, EntityPrototypeId, ItemId, PrototypeCatalog};
use factory_sim::{CHUNK_SIZE, Direction, EntityFootprint, Inventory, ItemStack, Simulation};
use std::time::Duration;

const TARGET_TICKS: u64 = 3_600;

#[test]
fn fixed_update_hash_matches_at_60_and_144_fps() {
    let at_60_fps = run_to_tick_with_frame_rate(60.0, TARGET_TICKS);
    let at_144_fps = run_to_tick_with_frame_rate(144.0, TARGET_TICKS);

    assert_eq!(at_60_fps.0, TARGET_TICKS);
    assert_eq!(at_144_fps.0, TARGET_TICKS);
    assert_eq!(at_60_fps.1, at_144_fps.1);
}

#[test]
fn zero_duration_render_pause_does_not_advance_or_corrupt_sim() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    run_until_tick(&mut app, 120);

    let before_pause = sim_tick_and_hash(&app);
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));

    for _ in 0..240 {
        app.update();
    }

    assert_eq!(sim_tick_and_hash(&app), before_pause);

    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    run_until_tick(&mut app, TARGET_TICKS);

    let mut expected = Simulation::new_test_world(123);
    for _ in 0..TARGET_TICKS {
        expected.tick();
    }

    assert_eq!(
        sim_tick_and_hash(&app),
        (TARGET_TICKS, expected.state_hash())
    );
}

#[test]
fn world_position_to_tile_coord_floors_negative_coordinates() {
    assert_eq!(world_position_to_tile_coord(Vec2::new(0.0, 0.0)), (0, 0));
    assert_eq!(world_position_to_tile_coord(Vec2::new(7.99, 7.99)), (0, 0));
    assert_eq!(world_position_to_tile_coord(Vec2::new(8.0, 8.0)), (1, 1));
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-0.01, -0.01)),
        (-1, -1)
    );
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-8.0, -8.0)),
        (-1, -1)
    );
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-8.01, -8.01)),
        (-2, -2)
    );
}

#[test]
fn input_movement_changes_player_position_under_fixed_ticks() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let before = app.world().resource::<SimResource>().sim.player;
    let before_tick = app.world().resource::<SimResource>().sim.tick_count();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    run_until_tick(&mut app, before_tick + 1);

    let after = app.world().resource::<SimResource>().sim.player;
    assert!(after.x_fixed() > before.x_fixed());
    assert_eq!(after.y_fixed(), before.y_fixed());
}

#[test]
fn debug_inventory_keys_insert_and_remove_selected_item() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    let selected_item = app
        .world()
        .resource::<SimResource>()
        .sim
        .world
        .prototypes
        .items[0]
        .id;
    let before = app
        .world()
        .resource::<SimResource>()
        .sim
        .player_inventory
        .count(selected_item);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyI);
    app.update();

    let after_insert = app
        .world()
        .resource::<SimResource>()
        .sim
        .player_inventory
        .count(selected_item);
    assert_eq!(after_insert, before + 1);

    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.release(KeyCode::KeyI);
        keyboard.clear();
        keyboard.press(KeyCode::KeyO);
    }
    app.update();

    let after_remove = app
        .world()
        .resource::<SimResource>()
        .sim
        .player_inventory
        .count(selected_item);
    assert_eq!(after_remove, before);
}

#[test]
fn opening_clicked_chest_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_burner_drill_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_furnace_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_assembler_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&sim, assembler);
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_lab_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let (x, y) = first_buildable_rect(&sim, lab);
    let entity_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn assembler_recipe_choices_are_all_and_only_crafting_recipes() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let choices = crafting_recipe_choices(&catalog);
    let expected_count = catalog
        .recipes
        .iter()
        .filter(|recipe| recipe.category == CraftingCategory::Crafting)
        .count();

    assert_eq!(choices.len(), expected_count);
    assert!(
        choices
            .iter()
            .all(|recipe| recipe.category == CraftingCategory::Crafting)
    );
    assert!(
        catalog
            .recipes
            .iter()
            .filter(|recipe| recipe.category != CraftingCategory::Crafting)
            .all(|recipe| !choices.iter().any(|choice| choice.id == recipe.id))
    );
}

#[test]
fn available_crafting_recipe_choices_follow_research_unlocks() {
    let mut sim = Simulation::new_test_world(123);
    let automation = technology_id_by_name(&sim.world.prototypes, "automation");
    let assembling_machine = recipe_id_by_name(&sim.world.prototypes, "assembling_machine");

    let initial_choices = available_crafting_recipe_choices(&sim);
    assert!(
        !initial_choices
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );

    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.add_research_units(10)
        .expect("automation research should complete");

    let unlocked_choices = available_crafting_recipe_choices(&sim);
    assert!(
        unlocked_choices
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );
}

#[test]
fn completed_research_unlocks_recipe() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let automation = technology_id_by_name(&sim.world.prototypes, "automation");
    let science_pack = item_id_by_name(&sim.world.prototypes, "automation_science_pack");
    let assembling_machine = recipe_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&sim, lab);
    let lab_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.entity_inventory_mut(lab_id)
        .expect("lab should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 10,
    });

    assert!(
        !available_crafting_recipe_choices(&sim)
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );

    for _ in 0..6_000 {
        sim.tick();
    }

    assert!(
        available_crafting_recipe_choices(&sim)
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );
}

#[test]
fn locked_assembler_recipe_buttons_are_unavailable_without_error() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let recipe = recipe_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&sim, assembler);
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");

    assert_eq!(
        sim.can_select_assembler_recipe(entity_id, recipe),
        Ok(false)
    );
}

#[test]
fn assembler_detail_formatting_reports_partial_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let recipe = recipe_id_by_name(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim, assembler);
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    sim.select_assembler_recipe(entity_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 1,
    });
    sim.transfer_player_slot_to_assembler_input(entity_id, 2)
        .expect("partial ingredients should transfer to assembler input");

    let details =
        format_assembler_detail_text(&sim, entity_id).expect("assembler details should format");

    assert_eq!(details.recipe, "Recipe: Iron Gear Wheel");
    assert_eq!(
        details.ingredients,
        "Ingredients:\nIron Plate: need 2, have 1, missing 1"
    );
    assert_eq!(details.products, "Output: Iron Gear Wheel x1");
    assert_eq!(details.progress, "Progress: 0/60");
}

#[test]
fn slot_click_transfer_delegates_to_sim_transfer_api() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 9,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer stack to chest");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        9
    );
}

#[test]
fn slot_click_transfer_routes_science_to_lab_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let science_pack = item_id_by_name(&sim.world.prototypes, "automation_science_pack");
    let (x, y) = first_buildable_rect(&sim, lab);
    let entity_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: science_pack,
        count: 3,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer science packs to lab");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(
        sim.entity_inventory(entity_id)
            .expect("lab should expose inventory")
            .count(science_pack),
        3
    );
}

#[test]
fn slot_click_transfer_routes_furnace_input_fuel_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let iron_ore = item_id_by_name(&sim.world.prototypes, "iron_ore");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.player_inventory.slots[3] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player ore should transfer to furnace input");
    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 3)
        .expect("player coal should transfer to furnace fuel");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(sim.player_inventory.slots[3], None);
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .input_slot,
        Some(ItemStack {
            item_id: iron_ore,
            count: 1,
        })
    );
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .energy
            .fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );

    for _ in 0..210 {
        sim.tick();
    }

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::FurnaceOutput, 0)
        .expect("furnace output should transfer to player");

    assert_eq!(sim.player_inventory.count(iron_plate), 1);
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .output_slot,
        None
    );
}

#[test]
fn slot_click_transfer_routes_assembler_input_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let recipe = recipe_id_by_name(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id_by_name(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id_by_name(&sim.world.prototypes, "iron_gear_wheel");
    let (x, y) = first_buildable_rect(&sim, assembler);
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    sim.select_assembler_recipe(entity_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player ingredients should transfer to assembler input");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(
        sim.assembler_state(entity_id)
            .expect("assembler should expose state")
            .input_inventory
            .count(iron_plate),
        2
    );

    for _ in 0..60 {
        sim.tick();
    }

    transfer_open_container_slot(
        &mut sim,
        Some(entity_id),
        InventoryPanel::AssemblerOutput,
        0,
    )
    .expect("assembler output should transfer to player");

    assert_eq!(sim.player_inventory.count(iron_gear_wheel), 1);
    assert_eq!(
        sim.assembler_state(entity_id)
            .expect("assembler should expose state")
            .output_inventory
            .count(iron_gear_wheel),
        0
    );
}

#[test]
fn slot_click_rejects_invalid_furnace_input_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let inserter = item_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: inserter,
        count: 1,
    });

    assert!(
        transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2).is_err()
    );
    assert_eq!(
        sim.player_inventory.slots[2],
        Some(ItemStack {
            item_id: inserter,
            count: 1,
        })
    );
    assert_eq!(
        sim.furnace_state(entity_id)
            .expect("furnace should expose state")
            .input_slot,
        None
    );
}

#[test]
fn slot_click_transfer_handles_burner_drill_fuel_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let coal = item_id_by_name(&sim.world.prototypes, "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[2] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player coal should transfer to burner drill fuel");

    assert_eq!(sim.player_inventory.slots[2], None);
    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .energy
            .fuel_slot,
        Some(ItemStack {
            item_id: coal,
            count: 1,
        })
    );

    for _ in 0..240 {
        sim.tick();
    }

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::BurnerOutput, 0)
        .expect("drill output should transfer to player");

    assert_eq!(sim.player_inventory.count(coal), 1);
    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        None
    );
}

#[test]
fn debug_placement_key_places_transport_belt_with_current_direction() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let mut build_direction = DebugBuildDirection::default();
    let mut keyboard = ButtonInput::default();
    let (first_x, first_y) = first_buildable_rect(&sim, belt);

    keyboard.press(KeyCode::KeyT);
    let first_id = handle_debug_build_action_at_tile(
        &mut sim,
        &keyboard,
        &mut build_direction,
        first_x,
        first_y,
    )
    .expect("T should place a transport belt");

    let first = sim.entities.placed_entity(first_id).unwrap();
    assert_eq!(first.direction, Direction::North);
    assert_eq!(
        sim.world.prototypes.entities[first.prototype_id.index()].entity_kind,
        EntityKind::TransportBelt
    );

    keyboard.release(KeyCode::KeyT);
    keyboard.clear();
    keyboard.press(KeyCode::KeyR);
    handle_debug_build_action_at_tile(&mut sim, &keyboard, &mut build_direction, first_x, first_y);
    assert_eq!(build_direction.direction, Direction::East);

    keyboard.release(KeyCode::KeyR);
    keyboard.clear();
    keyboard.press(KeyCode::KeyT);
    let (second_x, second_y) = first_buildable_rect(&sim, belt);
    let second_id = handle_debug_build_action_at_tile(
        &mut sim,
        &keyboard,
        &mut build_direction,
        second_x,
        second_y,
    )
    .expect("T should place another transport belt");

    assert_eq!(
        sim.entities.placed_entity(second_id).unwrap().direction,
        Direction::East
    );
}

#[test]
fn debug_placement_key_places_assembler() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let mut build_direction = DebugBuildDirection::default();
    let mut keyboard = ButtonInput::default();
    let (x, y) = first_buildable_rect(&sim, assembler);

    keyboard.press(KeyCode::KeyA);
    let entity_id =
        handle_debug_build_action_at_tile(&mut sim, &keyboard, &mut build_direction, x, y)
            .expect("A should place an assembler");

    assert_eq!(
        sim.world.prototypes.entities[sim
            .entities
            .placed_entity(entity_id)
            .expect("placed assembler should remain")
            .prototype_id
            .index()]
        .entity_kind,
        EntityKind::AssemblingMachine
    );
    assert!(sim.assembler_state(entity_id).is_ok());
}

#[test]
fn debug_placement_key_places_lab() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let mut build_direction = DebugBuildDirection::default();
    let mut keyboard = ButtonInput::default();
    let (x, y) = first_buildable_rect(&sim, lab);

    keyboard.press(KeyCode::KeyL);
    let entity_id =
        handle_debug_build_action_at_tile(&mut sim, &keyboard, &mut build_direction, x, y)
            .expect("L should place a lab");

    assert_eq!(
        sim.world.prototypes.entities[sim
            .entities
            .placed_entity(entity_id)
            .expect("placed lab should remain")
            .prototype_id
            .index()]
        .entity_kind,
        EntityKind::Lab
    );
    assert!(sim.lab_state(entity_id).is_ok());
}

#[test]
fn debug_belt_insert_key_adds_selected_item_to_clicked_belt() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    let iron_ore = item_id_by_name(&sim.world.prototypes, "iron_ore");
    let (x, y) = first_buildable_rect(&sim, belt);
    let belt_id = sim
        .place_entity(belt, x, y, Direction::East)
        .expect("belt should be placeable");
    let inventory_selection = factory_app::DebugInventorySelection { selected_index: 0 };

    handle_debug_belt_item_insertion_at_tile(&mut sim, &inventory_selection, x, y)
        .expect("clicked belt should accept selected debug item");

    assert!(
        sim.belt_segment(belt_id)
            .expect("belt should expose segment state")
            .lanes
            .iter()
            .any(|lane| lane.items.iter().any(|item| item.item_id == iron_ore))
    );
}

fn run_to_tick_with_frame_rate(frame_rate: f64, target_tick: u64) -> (u64, u64) {
    let mut app = test_app(Duration::from_secs_f64(1.0 / frame_rate));
    run_until_tick(&mut app, target_tick);
    sim_tick_and_hash(&app)
}

fn test_app(frame_duration: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration));
    app
}

fn run_until_tick(app: &mut App, target_tick: u64) {
    while app.world().resource::<SimResource>().sim.tick_count() < target_tick {
        app.update();
    }
}

fn sim_tick_and_hash(app: &App) -> (u64, u64) {
    let sim = &app.world().resource::<SimResource>().sim;
    (sim.tick_count(), sim.state_hash())
}

fn first_buildable_rect(sim: &Simulation, prototype_id: EntityPrototypeId) -> (i32, i32) {
    let prototype = &sim.world.prototypes.entities[prototype_id.index()];

    for chunk in sim.world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;
            let footprint = EntityFootprint {
                x,
                y,
                width: prototype.size.x,
                height: prototype.size.y,
            };

            if sim.world.validate_entity_footprint(&footprint).is_ok()
                && sim
                    .entities
                    .occupancy()
                    .validate_available(&footprint, None)
                    .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one buildable area");
}

fn first_placeable_resource_rect(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    resource_item: ItemId,
) -> (i32, i32) {
    for chunk in sim.world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let Some(resource) = tile.resource else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }

            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;

            if sim
                .can_place_entity(prototype_id, x, y, Direction::North)
                .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected at least one placeable resource area");
}

fn entity_id_by_name(catalog: &PrototypeCatalog, name: &str) -> EntityPrototypeId {
    catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required entity prototype {name:?}"))
}

fn item_id_by_name(catalog: &PrototypeCatalog, name: &str) -> ItemId {
    catalog
        .items
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required item prototype {name:?}"))
}

fn recipe_id_by_name(catalog: &PrototypeCatalog, name: &str) -> factory_data::RecipeId {
    catalog
        .recipes
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required recipe prototype {name:?}"))
}

fn technology_id_by_name(catalog: &PrototypeCatalog, name: &str) -> factory_data::TechnologyId {
    catalog
        .technologies
        .iter()
        .find(|prototype| prototype.name == name)
        .map(|prototype| prototype.id)
        .unwrap_or_else(|| panic!("missing required technology prototype {name:?}"))
}
