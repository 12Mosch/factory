use super::super::*;
use super::support::*;

#[test]
fn apply_command_move_player_matches_direct_call() {
    let mut via_command = Simulation::new_test_world(123);
    let mut direct = Simulation::new_test_world(123);

    via_command
        .apply_command(&SimCommand::MovePlayer {
            direction_x: 1.0,
            direction_y: 0.0,
            delta_seconds: 0.5,
        })
        .expect("move should apply");
    direct.move_player(1.0, 0.0, 0.5);

    assert_eq!(via_command.player, direct.player);
}

#[test]
fn apply_command_set_manual_mining_target_matches_direct_call() {
    let mut via_command = Simulation::new_test_world(123);
    let mut direct = Simulation::new_test_world(123);
    let target = ManualMiningTarget {
        x: direct.player.tile_position().0,
        y: direct.player.tile_position().1,
    };

    via_command
        .apply_command(&SimCommand::SetManualMiningTarget(Some(target)))
        .expect("manual mining target should apply");
    direct.update_manual_mining(Some(target));

    assert_eq!(
        via_command.manual_mining_progress,
        direct.manual_mining_progress
    );
}

#[test]
fn apply_command_reports_manually_mined_item_and_inventory_total() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, resource) = first_resource_tile(&sim.world);
    let target = ManualMiningTarget { x, y };
    sim.player = PlayerState::centered_on_tile(x, y);
    let count_before = sim.player_inventory.count(resource.resource_item);

    let mut effect = SimCommandEffect::None;
    for _ in 0..MANUAL_MINING_TICKS_PER_ITEM {
        effect = sim
            .apply_command(&SimCommand::SetManualMiningTarget(Some(target)))
            .expect("manual mining target should apply");
    }

    assert_eq!(
        effect,
        SimCommandEffect::PlayerItemGained {
            item_id: resource.resource_item,
            amount: 1,
            total: count_before + 1,
        }
    );
}

#[test]
fn apply_command_reports_deconstructed_item_and_inventory_total() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let chest_item = item_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = place_at(&mut sim, chest, x, y, Direction::North);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, entity_id)
            .expect("chest should expose its inventory"),
        0,
        chest_item,
        1,
    );
    let count_before = sim.player_inventory.count(chest_item);

    sim.apply_command(&SimCommand::MarkDeconstruction {
        min_x: x,
        min_y: y,
        max_x: x + 1,
        max_y: y + 1,
    })
    .expect("entity should be marked for deconstruction");
    let effect = sim
        .apply_command(&SimCommand::DeconstructEntity { entity_id })
        .expect("marked entity should deconstruct");

    assert_eq!(
        effect,
        SimCommandEffect::PlayerItemGained {
            item_id: chest_item,
            amount: 2,
            total: count_before + 2,
        }
    );
}

#[test]
fn apply_command_start_manual_craft_consumes_ingredients() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, iron_plate, 2)
        .expect("test inventory should accept ingredients");

    sim.apply_command(&SimCommand::StartManualCraft(recipe))
        .expect("craft should start with enough ingredients");

    assert_eq!(sim.player_inventory.count(iron_plate), 0);
    assert_eq!(sim.crafting_queue.entries.len(), 1);
}

#[test]
fn apply_command_start_manual_craft_reports_crafting_error() {
    let mut sim = Simulation::new_test_world(123);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    sim.player_inventory = Inventory::player();

    let error = sim
        .apply_command(&SimCommand::StartManualCraft(recipe))
        .expect_err("craft should fail without ingredients");

    assert_eq!(
        error,
        SimCommandError::Crafting(CraftingError::InsufficientIngredients)
    );
}

#[test]
fn apply_command_research_queue_lifecycle() {
    let mut sim = Simulation::new_test_world(123);
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let automation = technology_id(&sim.world.prototypes, "automation");

    sim.apply_command(&SimCommand::EnqueueResearch(logistics))
        .expect("logistics should be enqueueable");
    sim.apply_command(&SimCommand::EnqueueResearch(automation))
        .expect("automation should be enqueueable after logistics");
    assert_eq!(sim.research.active, Some(logistics));
    assert_eq!(sim.research.queue, vec![automation]);

    sim.apply_command(&SimCommand::MoveQueuedResearch {
        from_index: 0,
        to_index: 0,
    })
    .expect("moving to the same index should be a no-op");

    sim.apply_command(&SimCommand::RemoveQueuedResearch { index: 0 })
        .expect("queued research should be removable");
    assert!(sim.research.queue.is_empty());
}

#[test]
fn apply_command_select_assembler_recipe_routes_through_sim() {
    let mut sim = Simulation::new_test_world(123);
    let entity_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.apply_command(&SimCommand::SelectAssemblerRecipe {
        entity_id,
        recipe_id: recipe,
    })
    .expect("recipe should be selectable");

    assert_eq!(
        crate::entity_access::assembler_state(&sim, entity_id)
            .expect("assembler should expose state")
            .selected_recipe,
        Some(recipe)
    );
}

#[test]
fn apply_command_transfer_slot_routes_player_input_by_machine_kind() {
    let mut sim = Simulation::new_test_world(123);
    let entity_id = place_stone_furnace(&mut sim);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let coal = item_id(&sim.world.prototypes, "coal");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_ore, 1);
    set_inventory_slot(&mut sim.player_inventory, 1, coal, 1);

    sim.apply_command(&SimCommand::TransferSlot {
        entity_id,
        panel: InventoryPanel::Player,
        slot_index: 0,
    })
    .expect("ore should route to furnace input");
    sim.apply_command(&SimCommand::TransferSlot {
        entity_id,
        panel: InventoryPanel::Player,
        slot_index: 1,
    })
    .expect("coal should route to furnace fuel");

    let furnace_state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(
        furnace_state.input_slot.stack(),
        Some(test_stack(iron_ore, 1))
    );
    assert_eq!(
        furnace_state
            .energy
            .fuel_slot()
            .expect("burner furnace")
            .stack(),
        Some(test_stack(coal, 1))
    );
}

#[test]
fn apply_command_transfer_slot_reports_typed_error() {
    let mut sim = Simulation::new_test_world(123);
    let entity_id = place_stone_furnace(&mut sim);
    let inserter = item_id(&sim.world.prototypes, "inserter");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, inserter, 1);

    let error = sim
        .apply_command(&SimCommand::TransferSlot {
            entity_id,
            panel: InventoryPanel::Player,
            slot_index: 0,
        })
        .expect_err("inserter is not valid furnace input or fuel");

    assert_eq!(
        error,
        SimCommandError::Transfer(SlotTransferError::Furnace(FurnaceError::InvalidInput(
            inserter
        )))
    );
}

#[test]
fn apply_command_place_entity_from_player_inventory_reports_entity_id() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let chest_item = item_id(&sim.world.prototypes, "chest");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    sim.player_inventory = Inventory::player();
    sim.player_inventory
        .insert(&sim.world.prototypes, chest_item, 1)
        .expect("test inventory should accept a chest");

    let effect = sim
        .apply_command(&SimCommand::PlaceEntityFromPlayerInventory {
            prototype_id: chest,
            item_id: chest_item,
            x,
            y,
            direction: Direction::North,
        })
        .expect("chest should be placeable");

    let SimCommandEffect::EntityPlaced(entity_id) = effect else {
        panic!("placing an entity should report SimCommandEffect::EntityPlaced");
    };
    assert!(crate::entity_access::inventory(&sim, entity_id).is_ok());
    assert_eq!(sim.player_inventory.count(chest_item), 0);
}
