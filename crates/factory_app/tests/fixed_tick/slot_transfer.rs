use super::common::{
    entity_id_by_name, first_buildable_rect, first_placeable_resource_rect, item_id_by_name,
    place_powered_fixture_origin, recipe_id_by_name,
};
use factory_app::interaction::slot_transfer::transfer_open_container_slot;
use factory_app::ui::inventory_panel::InventoryPanel;
use factory_sim::{Direction, Inventory, ItemStack, Simulation};

#[test]
fn slot_click_transfer_delegates_to_sim_transfer_api() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(sim.catalog(), "chest");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 9,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer stack to chest");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
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
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let science_pack = item_id_by_name(sim.catalog(), "automation_science_pack");
    let (x, y) = first_buildable_rect(&sim, lab);
    let entity_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: science_pack,
        count: 3,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("slot click should transfer science packs to lab");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
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
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let iron_ore = item_id_by_name(sim.catalog(), "iron_ore");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });
    sim.player_inventory_mut().slots[3] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player ore should transfer to furnace input");
    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 3)
        .expect("player coal should transfer to furnace fuel");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
    assert_eq!(sim.player_inventory_mut().slots[3], None);
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

    assert_eq!(sim.player_inventory().count(iron_plate), 1);
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
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let recipe = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let iron_gear_wheel = item_id_by_name(sim.catalog(), "iron_gear_wheel");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    sim.select_assembler_recipe(entity_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player ingredients should transfer to assembler input");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
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

    assert_eq!(sim.player_inventory().count(iron_gear_wheel), 1);
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
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let inserter = item_id_by_name(sim.catalog(), "inserter");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: inserter,
        count: 1,
    });

    assert!(
        transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2).is_err()
    );
    assert_eq!(
        sim.player_inventory_mut().slots[2],
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
    let drill = entity_id_by_name(sim.catalog(), "burner_mining_drill");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");
    *sim.player_inventory_mut() = Inventory::player();
    sim.player_inventory_mut().slots[2] = Some(ItemStack {
        item_id: coal,
        count: 1,
    });

    transfer_open_container_slot(&mut sim, Some(entity_id), InventoryPanel::Player, 2)
        .expect("player coal should transfer to burner drill fuel");

    assert_eq!(sim.player_inventory_mut().slots[2], None);
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

    assert_eq!(sim.player_inventory().count(coal), 1);
    assert_eq!(
        sim.burner_drill_state(entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        None
    );
}
