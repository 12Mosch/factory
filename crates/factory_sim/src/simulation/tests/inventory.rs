use super::super::*;
use super::support::*;

#[test]
fn chest_inventory_accepts_items() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");
    let catalog = sim.world.prototypes.clone();

    crate::entity_access::inventory_mut(&mut sim, entity_id)
        .expect("chest should expose mutable inventory")
        .insert(&catalog, iron_plate, 25)
        .expect("chest should accept iron plates");

    assert_eq!(
        crate::entity_access::inventory(&sim, entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        25
    );
}

#[test]
fn player_can_transfer_stack_to_chest() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 5, iron_plate, 42);

    crate::entity_transfer::player_slot_to_entity(&mut sim, entity_id, 5)
        .expect("stack should transfer to chest");

    assert_eq!(sim.player_inventory.slots()[5], None);
    assert_eq!(
        crate::entity_access::inventory(&sim, entity_id)
            .expect("chest should have inventory")
            .count(iron_plate),
        42
    );
}

#[test]
fn transfer_to_full_chest_fails_without_changing_player_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 3, iron_plate, 12);
    {
        let catalog = sim.world.prototypes.clone();
        let inventory = crate::entity_access::inventory_mut(&mut sim, entity_id)
            .expect("chest should expose inventory");
        let stack =
            ItemStack::new(&catalog, coal, 100).expect("coal should form a full valid stack");
        *inventory =
            Inventory::from_slots(&catalog, vec![test_slot(stack); inventory.slots().len()])
                .expect("full chest fixture should be valid");
    }
    assert!(
        !crate::entity_access::inventory(&sim, entity_id)
            .expect("chest should have inventory")
            .can_insert(&sim.world.prototypes, iron_plate, 12)
    );
    let player_before = sim.player_inventory.clone();

    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, entity_id, 3),
        Err(ContainerError::InsufficientSpace)
    );
    assert_eq!(sim.player_inventory, player_before);
}

#[test]
fn transfer_from_chest_to_full_player_fails_without_changing_chest_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let coal = item_id(&sim.world.prototypes, "coal");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable");
    sim.player_inventory = Inventory::with_slot_count(1);
    sim.player_inventory
        .insert(&sim.world.prototypes, coal, 100)
        .expect("player inventory should accept blocking stack");
    let inventory = crate::entity_access::inventory_mut(&mut sim, entity_id)
        .expect("chest should expose inventory");
    set_inventory_slot(inventory, 0, iron_plate, 8);
    let chest_before = crate::entity_access::inventory(&sim, entity_id)
        .expect("chest should have inventory")
        .clone();

    assert_eq!(
        crate::entity_transfer::entity_slot_to_player(&mut sim, entity_id, 0),
        Err(ContainerError::InsufficientSpace)
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, entity_id)
            .expect("chest should still have inventory"),
        &chest_before
    );
}

#[test]
fn non_container_entities_reject_inventory_access() {
    let mut sim = Simulation::new_test_world(123);
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let entity_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: inserter,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("inserter should be placeable");

    assert_eq!(
        crate::entity_access::inventory(&sim, entity_id),
        Err(ContainerError::NotContainer(entity_id))
    );
}

#[test]
fn lab_rejects_non_science_pack_player_transfer_without_mutation() {
    let mut sim = Simulation::new_test_world(123);
    let lab_id = place_lab(&mut sim);
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 1);

    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, lab_id, 0),
        Err(ContainerError::InvalidItem(iron_plate))
    );
    assert_eq!(
        sim.player_inventory.slots()[0],
        Some(test_stack(iron_plate, 1))
    );
    assert_eq!(
        crate::entity_access::inventory(&sim, lab_id)
            .expect("lab should expose inventory")
            .count(iron_plate),
        0
    );
}

#[test]
fn inventory_merges_stacks_until_stack_size() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let iron_plate = item_id(&catalog, "iron_plate");
    let mut inventory = Inventory::with_slot_count(2);

    inventory
        .insert(&catalog, iron_plate, 99)
        .expect("first insert should fit");
    inventory
        .insert(&catalog, iron_plate, 2)
        .expect("second insert should fill existing stack first");

    assert_eq!(
        inventory.slots(),
        vec![
            Some(test_stack(iron_plate, 100)),
            Some(test_stack(iron_plate, 1)),
        ]
    );
}

#[test]
fn inventory_rejects_insert_when_full() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let iron_plate = item_id(&catalog, "iron_plate");
    let coal = item_id(&catalog, "coal");
    let mut inventory = Inventory::with_slot_count(1);

    inventory
        .insert(&catalog, iron_plate, 100)
        .expect("initial stack should fit");
    let before = inventory.clone();

    assert_eq!(
        inventory.insert(&catalog, coal, 1),
        Err(InventoryError::InsufficientSpace)
    );
    assert_eq!(inventory, before);
}

#[test]
fn inventory_acceptance_reports_unknown_items() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let mut inventory = Inventory::with_slot_count(1);
    let unknown_item = ItemId::new(catalog.items.len() as u16);

    assert_eq!(
        inventory.insert(&catalog, unknown_item, 1),
        Err(InventoryError::UnknownItem(unknown_item))
    );
}

#[test]
fn inventory_remove_is_atomic() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let iron_plate = item_id(&catalog, "iron_plate");
    let mut inventory = Inventory::with_slot_count(1);

    inventory
        .insert(&catalog, iron_plate, 3)
        .expect("initial stack should fit");
    let before = inventory.clone();

    assert_eq!(
        inventory.remove(iron_plate, 4),
        Err(InventoryError::InsufficientItems)
    );
    assert_eq!(inventory, before);
    assert_eq!(inventory.count(iron_plate), 3);
}

#[test]
fn player_starts_with_drill_and_furnace_only() {
    let sim = Simulation::new_test_world(123);
    let burner_mining_drill = item_id(&sim.world.prototypes, "burner_mining_drill");
    let stone_furnace = item_id(&sim.world.prototypes, "stone_furnace");
    let occupied_slots = sim
        .player_inventory
        .slots()
        .iter()
        .filter_map(|slot| slot.stack())
        .collect::<Vec<_>>();

    assert_eq!(
        sim.player_inventory.slots().len(),
        PLAYER_INVENTORY_SLOT_COUNT
    );
    assert_eq!(sim.player_inventory.count(burner_mining_drill), 1);
    assert_eq!(sim.player_inventory.count(stone_furnace), 1);
    assert_eq!(occupied_slots.len(), 2);
    assert_eq!(
        occupied_slots
            .iter()
            .map(|stack| stack.count())
            .sum::<u16>(),
        2
    );
}

#[test]
fn inventory_insert_never_exceeds_item_stack_size() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let copper_cable = item_id(&catalog, "copper_cable");
    let mut inventory = Inventory::with_slot_count(2);

    inventory
        .insert(&catalog, copper_cable, 201)
        .expect("two cable stacks should fit");

    assert_eq!(inventory.count(copper_cable), 201);
    for stack in inventory.slots().iter().filter_map(|slot| slot.stack()) {
        assert!(stack.count() <= 200);
    }
}

#[test]
fn zero_count_insert_and_remove_are_no_ops() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let unknown_item = ItemId::new(u16::MAX);
    let mut inventory = Inventory::with_slot_count(1);

    inventory
        .insert(&catalog, unknown_item, 0)
        .expect("zero-count insert should be a no-op");
    inventory
        .remove(unknown_item, 0)
        .expect("zero-count remove should be a no-op");

    assert_eq!(inventory.slots(), vec![None]);
}
