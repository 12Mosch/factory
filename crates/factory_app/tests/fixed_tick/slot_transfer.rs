use super::common::{
    entity_id_by_name, first_buildable_rect, first_placeable_resource_rect, item_id_by_name,
    place_powered_fixture_origin, place_test_entity, recipe_id_by_name, set_player_inventory_slot,
};
use bevy::prelude::*;
use factory_app::resources::SimResource;
use factory_app::ui::inventory_panel::{InventoryPanel, slot_transfer_error_message};
use factory_app::ui::resources::{InventoryTransferFeedback, OpenContainer};
use factory_sim::{
    ContainerError, FurnaceError, Inventory, ItemStack, Simulation, SlotTransferError,
};

#[test]
fn slot_click_transfer_delegates_to_sim_transfer_api() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(sim.catalog(), "chest");
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = place_test_entity(&mut sim, chest, x, y);
    *sim.player_inventory_mut() = Inventory::player();
    set_player_inventory_slot(&mut sim, 2, iron_plate, 9);

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::Player,
        2,
    )
    .expect("slot click should transfer stack to chest");

    assert_eq!(sim.player_inventory().slots()[2], None);
    assert_eq!(
        factory_sim::entity_access::inventory(&sim, entity_id)
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
    let entity_id = place_test_entity(&mut sim, lab, x, y);
    *sim.player_inventory_mut() = Inventory::player();
    set_player_inventory_slot(&mut sim, 2, science_pack, 3);

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::Player,
        2,
    )
    .expect("slot click should transfer science packs to lab");

    assert_eq!(sim.player_inventory().slots()[2], None);
    assert_eq!(
        factory_sim::entity_access::inventory(&sim, entity_id)
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
    let entity_id = place_test_entity(&mut sim, furnace, x, y);
    *sim.player_inventory_mut() = Inventory::player();
    set_player_inventory_slot(&mut sim, 2, iron_ore, 1);
    set_player_inventory_slot(&mut sim, 3, coal, 1);

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::Player,
        2,
    )
    .expect("player ore should transfer to furnace input");
    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::Player,
        3,
    )
    .expect("player coal should transfer to furnace fuel");

    assert_eq!(sim.player_inventory().slots()[2], None);
    assert_eq!(sim.player_inventory().slots()[3], None);
    assert_eq!(
        factory_sim::entity_access::furnace_state(&sim, entity_id)
            .expect("furnace should expose state")
            .input_slot,
        Some(ItemStack::new(sim.catalog(), iron_ore, 1).expect("expected valid test stack"))
    );
    assert_eq!(
        factory_sim::entity_access::furnace_state(&sim, entity_id)
            .expect("furnace should expose state")
            .energy
            .fuel_slot,
        Some(ItemStack::new(sim.catalog(), coal, 1).expect("expected valid test stack"))
    );

    for _ in 0..210 {
        sim.tick();
    }

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::FurnaceOutput,
        0,
    )
    .expect("furnace output should transfer to player");

    assert_eq!(sim.player_inventory().count(iron_plate), 1);
    assert_eq!(
        factory_sim::entity_access::furnace_state(&sim, entity_id)
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
    let entity_id = place_test_entity(&mut sim, assembler, x, y);
    sim.select_assembler_recipe(entity_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    *sim.player_inventory_mut() = Inventory::player();
    set_player_inventory_slot(&mut sim, 2, iron_plate, 2);

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::Player,
        2,
    )
    .expect("player ingredients should transfer to assembler input");

    assert_eq!(sim.player_inventory().slots()[2], None);
    assert_eq!(
        factory_sim::entity_access::assembler_state(&sim, entity_id)
            .expect("assembler should expose state")
            .input_inventory
            .count(iron_plate),
        2
    );

    for _ in 0..60 {
        sim.tick();
    }

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::AssemblerOutput,
        0,
    )
    .expect("assembler output should transfer to player");

    assert_eq!(sim.player_inventory().count(iron_gear_wheel), 1);
    assert_eq!(
        factory_sim::entity_access::assembler_state(&sim, entity_id)
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
    let entity_id = place_test_entity(&mut sim, furnace, x, y);
    *sim.player_inventory_mut() = Inventory::player();
    set_player_inventory_slot(&mut sim, 2, inserter, 1);

    assert!(
        factory_sim::entity_transfer::transfer_container_slot(
            &mut sim,
            entity_id,
            InventoryPanel::Player,
            2
        )
        .is_err()
    );
    assert_eq!(
        sim.player_inventory().slots()[2],
        Some(ItemStack::new(sim.catalog(), inserter, 1).expect("expected valid test stack"))
    );
    assert_eq!(
        factory_sim::entity_access::furnace_state(&sim, entity_id)
            .expect("furnace should expose state")
            .input_slot,
        None
    );
}

#[test]
fn slot_click_error_message_formats_typed_transfer_errors() {
    let sim = Simulation::new_test_world(123);
    let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");

    assert_eq!(
        slot_transfer_error_message(
            sim.catalog(),
            SlotTransferError::Transfer(ContainerError::InvalidItem(iron_plate)),
        ),
        "Wrong item: Iron Plate"
    );
    assert_eq!(
        slot_transfer_error_message(
            sim.catalog(),
            SlotTransferError::Furnace(FurnaceError::InsufficientSpace),
        ),
        "No space"
    );
    assert_eq!(
        slot_transfer_error_message(
            sim.catalog(),
            SlotTransferError::Transfer(ContainerError::EmptySlot { slot_index: 3 }),
        ),
        "Empty slot"
    );
}

#[test]
fn slot_click_failure_updates_inventory_transfer_feedback() {
    let mut app = super::common::test_app(std::time::Duration::from_millis(16));
    let furnace = {
        let sim = &app.world().resource::<SimResource>().read();
        entity_id_by_name(sim.catalog(), "stone_furnace")
    };
    let inserter = {
        let sim = &app.world().resource::<SimResource>().read();
        item_id_by_name(sim.catalog(), "inserter")
    };
    let entity_id = {
        let mut sim_resource = app.world_mut().resource_mut::<SimResource>();
        let mut sim = sim_resource.write_for_tests();
        let (x, y) = first_buildable_rect(&sim, furnace);
        let entity_id = place_test_entity(&mut sim, furnace, x, y);
        *sim.player_inventory_mut() = Inventory::player();
        set_player_inventory_slot(&mut sim, 2, inserter, 1);
        entity_id
    };
    app.world_mut().resource_mut::<OpenContainer>().entity_id = Some(entity_id);

    app.update();
    app.update();
    press_button_with_child_text(&mut app, "I\n1");
    // The click queues a SimCommand in `Update`; the fixed tick that drains
    // it runs before `Update` on a later frame, so the effect is only
    // observable after a second `app.update()`.
    app.update();
    app.update();

    assert_eq!(
        app.world()
            .resource::<InventoryTransferFeedback>()
            .message
            .as_deref(),
        Some("Wrong item: Inserter")
    );
}

#[test]
fn slot_click_transfer_handles_burner_drill_fuel_and_output() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(sim.catalog(), "burner_mining_drill");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = place_test_entity(&mut sim, drill, x, y);
    *sim.player_inventory_mut() = Inventory::player();
    set_player_inventory_slot(&mut sim, 2, coal, 1);

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::Player,
        2,
    )
    .expect("player coal should transfer to burner drill fuel");

    assert_eq!(sim.player_inventory().slots()[2], None);
    assert_eq!(
        factory_sim::entity_access::burner_drill_state(&sim, entity_id)
            .expect("burner drill should expose state")
            .energy
            .fuel_slot,
        Some(ItemStack::new(sim.catalog(), coal, 1).expect("expected valid test stack"))
    );

    for _ in 0..240 {
        sim.tick();
    }

    factory_sim::entity_transfer::transfer_container_slot(
        &mut sim,
        entity_id,
        InventoryPanel::BurnerOutput,
        0,
    )
    .expect("drill output should transfer to player");

    assert_eq!(sim.player_inventory().count(coal), 1);
    assert_eq!(
        factory_sim::entity_access::burner_drill_state(&sim, entity_id)
            .expect("burner drill should expose state")
            .output_slot,
        None
    );
}

fn press_button_with_child_text(app: &mut App, target_text: &str) {
    let matching_button = {
        let world = app.world_mut();
        let mut buttons = world.query_filtered::<(Entity, &Children), With<Button>>();
        let mut texts = world.query::<&Text>();
        buttons
            .iter(world)
            .find_map(|(entity, children)| {
                children
                    .iter()
                    .any(|child| {
                        texts
                            .get(world, child)
                            .is_ok_and(|text| text.0 == target_text)
                    })
                    .then_some(entity)
            })
            .unwrap_or_else(|| {
                let labels = buttons
                    .iter(world)
                    .map(|(_, children)| {
                        children
                            .iter()
                            .filter_map(|child| texts.get(world, child).ok())
                            .map(|text| text.0.clone())
                            .collect::<Vec<_>>()
                            .join("|")
                    })
                    .collect::<Vec<_>>();
                panic!("expected matching slot button; labels: {labels:?}");
            })
    };

    let mut entity = app.world_mut().entity_mut(matching_button);
    let mut interaction = entity
        .get_mut::<Interaction>()
        .expect("button should have interaction state");
    *interaction = Interaction::Pressed;
}
