use super::super::*;
use super::support::*;

fn one_slot_inventory(catalog: &PrototypeCatalog, item_id: ItemId, count: u16) -> Inventory {
    Inventory::from_slots(
        catalog,
        vec![test_slot(
            ItemStack::new(catalog, item_id, count).expect("test stack should be valid"),
        )],
    )
    .expect("one-slot test inventory should be valid")
}

fn place_chest(sim: &mut Simulation) -> EntityId {
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chest,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chest should be placeable")
}

#[test]
fn turret_transfer_uses_item_acceptance_and_partial_inventory_capacity() {
    let mut sim = Simulation::new_test_world(123);
    let catalog = sim.world.prototypes.clone();
    let turret = entity_id_by_name(&catalog, "gun_turret");
    let ammo = item_id(&catalog, "firearm_magazine");
    let iron_plate = item_id(&catalog, "iron_plate");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    let turret_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: turret,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("gun turret should be placeable");

    *crate::entity_access::inventory_mut(&mut sim, turret_id).unwrap() =
        one_slot_inventory(&catalog, ammo, 198);
    sim.player_inventory = one_slot_inventory(&catalog, ammo, 5);
    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, turret_id, 0).unwrap(),
        TransferOutcome { moved_quantity: 2 }
    );
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 3);

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 1);
    let before = sim.state_hash();
    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, turret_id, 0),
        Err(ContainerError::InvalidItem(iron_plate))
    );
    assert_eq!(sim.state_hash(), before);
}

#[test]
fn container_transfers_report_full_and_partial_quantities_in_both_directions() {
    let mut sim = Simulation::new_test_world(123);
    let chest_id = place_chest(&mut sim);
    let catalog = sim.world.prototypes.clone();
    let iron_plate = item_id(&catalog, "iron_plate");

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 10);
    let outcome = crate::entity_transfer::player_slot_to_entity(&mut sim, chest_id, 0)
        .expect("the full player stack should transfer");
    assert_eq!(outcome, TransferOutcome { moved_quantity: 10 });
    assert_eq!(sim.player_inventory.slot(0), None);
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .unwrap()
            .count(iron_plate),
        10
    );

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 10);
    *crate::entity_access::inventory_mut(&mut sim, chest_id).unwrap() =
        one_slot_inventory(&catalog, iron_plate, 95);
    let outcome = crate::entity_transfer::player_slot_to_entity(&mut sim, chest_id, 0)
        .expect("the destination should accept its remaining capacity");
    assert_eq!(outcome, TransferOutcome { moved_quantity: 5 });
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 5);
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .unwrap()
            .slot(0)
            .unwrap()
            .count(),
        100
    );

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 97);
    *crate::entity_access::inventory_mut(&mut sim, chest_id).unwrap() =
        one_slot_inventory(&catalog, iron_plate, 10);
    let outcome = crate::entity_transfer::entity_slot_to_player(&mut sim, chest_id, 0)
        .expect("the player should accept its remaining capacity");
    assert_eq!(outcome, TransferOutcome { moved_quantity: 3 });
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 100);
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .unwrap()
            .slot(0)
            .unwrap()
            .count(),
        7
    );
}

#[test]
fn burner_slot_transfers_are_partial_and_preserve_exact_source_remainders() {
    let mut sim = Simulation::new_test_world(123);
    let catalog = sim.world.prototypes.clone();
    let coal = item_id(&catalog, "coal");
    let (drill_id, _, _, _) = place_burner_drill_on_resource(&mut sim, coal);

    sim.player_inventory = one_slot_inventory(&catalog, coal, 10);
    sim.entities
        .burner_drill_state_mut(drill_id)
        .unwrap()
        .energy
        .fuel_slot = test_slot(ItemStack::new(&catalog, coal, 95).unwrap());
    let outcome = crate::entity_transfer::player_slot_to_burner_drill_fuel(&mut sim, drill_id, 0)
        .expect("fuel slot should accept its remaining capacity");
    assert_eq!(outcome, TransferOutcome { moved_quantity: 5 });
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 5);
    assert_eq!(
        sim.entities
            .burner_drill_state(drill_id)
            .unwrap()
            .energy
            .fuel_slot
            .stack()
            .unwrap()
            .count(),
        100
    );

    sim.player_inventory = one_slot_inventory(&catalog, coal, 98);
    sim.entities
        .burner_drill_state_mut(drill_id)
        .unwrap()
        .energy
        .fuel_slot = test_slot(ItemStack::new(&catalog, coal, 10).unwrap());
    let outcome = crate::entity_transfer::burner_drill_fuel_to_player(&mut sim, drill_id)
        .expect("player should accept part of the fuel slot");
    assert_eq!(outcome, TransferOutcome { moved_quantity: 2 });
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 100);
    assert_eq!(
        sim.entities
            .burner_drill_state(drill_id)
            .unwrap()
            .energy
            .fuel_slot
            .stack()
            .unwrap()
            .count(),
        8
    );

    sim.player_inventory = one_slot_inventory(&catalog, coal, 99);
    sim.entities
        .burner_drill_state_mut(drill_id)
        .unwrap()
        .output_slot = test_slot(ItemStack::new(&catalog, coal, 10).unwrap());
    let outcome = crate::entity_transfer::burner_drill_output_to_player(&mut sim, drill_id)
        .expect("player should accept part of the output slot");
    assert_eq!(outcome, TransferOutcome { moved_quantity: 1 });
    assert_eq!(
        sim.entities
            .burner_drill_state(drill_id)
            .unwrap()
            .output_slot
            .stack()
            .unwrap()
            .count(),
        9
    );
}

#[test]
fn furnace_slots_transfer_partially_in_every_supported_direction() {
    let mut sim = Simulation::new_test_world(123);
    let catalog = sim.world.prototypes.clone();
    let iron_ore = item_id(&catalog, "iron_ore");
    let coal = item_id(&catalog, "coal");
    let iron_plate = item_id(&catalog, "iron_plate");
    let furnace_id = place_stone_furnace(&mut sim);

    sim.player_inventory = Inventory::with_slot_count(2);
    set_inventory_slot(&mut sim.player_inventory, 0, iron_ore, 10);
    set_inventory_slot(&mut sim.player_inventory, 1, coal, 10);
    {
        let furnace = sim.entities.furnace_state_mut(furnace_id).unwrap();
        furnace.input_slot = test_slot(ItemStack::new(&catalog, iron_ore, 95).unwrap());
        furnace.energy.fuel_slot = test_slot(ItemStack::new(&catalog, coal, 95).unwrap());
    }

    assert_eq!(
        crate::entity_transfer::player_slot_to_furnace_input(&mut sim, furnace_id, 0).unwrap(),
        TransferOutcome { moved_quantity: 5 }
    );
    assert_eq!(
        crate::entity_transfer::player_slot_to_furnace_fuel(&mut sim, furnace_id, 1).unwrap(),
        TransferOutcome { moved_quantity: 5 }
    );
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 5);
    assert_eq!(sim.player_inventory.slot(1).unwrap().count(), 5);

    for (source_item, transfer) in [
        (
            iron_ore,
            crate::entity_transfer::furnace_input_to_player
                as fn(&mut Simulation, EntityId) -> Result<TransferOutcome, FurnaceError>,
        ),
        (coal, crate::entity_transfer::furnace_fuel_to_player),
        (iron_plate, crate::entity_transfer::furnace_output_to_player),
    ] {
        sim.player_inventory = one_slot_inventory(&catalog, source_item, 99);
        {
            let furnace = sim.entities.furnace_state_mut(furnace_id).unwrap();
            match source_item {
                item if item == iron_ore => {
                    furnace.input_slot = test_slot(ItemStack::new(&catalog, item, 10).unwrap())
                }
                item if item == coal => {
                    furnace.energy.fuel_slot =
                        test_slot(ItemStack::new(&catalog, item, 10).unwrap())
                }
                item => {
                    furnace.output_slot = test_slot(ItemStack::new(&catalog, item, 10).unwrap())
                }
            }
        }

        assert_eq!(
            transfer(&mut sim, furnace_id).unwrap(),
            TransferOutcome { moved_quantity: 1 }
        );
        assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 100);
    }

    let furnace = sim.entities.furnace_state(furnace_id).unwrap();
    assert_eq!(furnace.input_slot.stack().unwrap().count(), 9);
    assert_eq!(furnace.energy.fuel_slot.stack().unwrap().count(), 9);
    assert_eq!(furnace.output_slot.stack().unwrap().count(), 9);
}

#[test]
fn assembler_inventories_transfer_partially_in_every_supported_direction() {
    let mut sim = Simulation::new_test_world(123);
    let catalog = sim.world.prototypes.clone();
    let iron_plate = item_id(&catalog, "iron_plate");
    let gear = item_id(&catalog, "iron_gear_wheel");
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&catalog, "iron_gear_wheel");
    sim.select_assembler_recipe(assembler_id, recipe).unwrap();

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 10);
    sim.entities
        .assembler_state_mut(assembler_id)
        .unwrap()
        .input_inventory = one_slot_inventory(&catalog, iron_plate, 95);
    assert_eq!(
        crate::entity_transfer::player_slot_to_assembler_input(&mut sim, assembler_id, 0).unwrap(),
        TransferOutcome { moved_quantity: 5 }
    );
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 5);

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 99);
    sim.entities
        .assembler_state_mut(assembler_id)
        .unwrap()
        .input_inventory = one_slot_inventory(&catalog, iron_plate, 10);
    assert_eq!(
        crate::entity_transfer::assembler_input_slot_to_player(&mut sim, assembler_id, 0).unwrap(),
        TransferOutcome { moved_quantity: 1 }
    );
    assert_eq!(
        sim.entities
            .assembler_state(assembler_id)
            .unwrap()
            .input_inventory
            .slot(0)
            .unwrap()
            .count(),
        9
    );

    sim.player_inventory = one_slot_inventory(&catalog, gear, 99);
    sim.entities
        .assembler_state_mut(assembler_id)
        .unwrap()
        .output_inventory = one_slot_inventory(&catalog, gear, 10);
    assert_eq!(
        crate::entity_transfer::assembler_output_slot_to_player(&mut sim, assembler_id, 0).unwrap(),
        TransferOutcome { moved_quantity: 1 }
    );
    assert_eq!(
        sim.entities
            .assembler_state(assembler_id)
            .unwrap()
            .output_inventory
            .slot(0)
            .unwrap()
            .count(),
        9
    );
}

#[test]
fn boiler_transfer_invalidates_dynamic_power_only_after_successful_commit() {
    let mut sim = Simulation::new_test_world(123);
    let catalog = sim.world.prototypes.clone();
    let coal = item_id(&catalog, "coal");
    let iron_plate = item_id(&catalog, "iron_plate");
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    sim.tick();
    sim.entities
        .boiler_state_mut(boiler_id)
        .unwrap()
        .energy
        .fuel_slot = test_slot(ItemStack::new(&catalog, coal, 50).unwrap());

    sim.player_inventory = one_slot_inventory(&catalog, iron_plate, 1);
    let before_hash = sim.state_hash();
    let power_before = sim.power.clone();
    assert_eq!(
        crate::entity_transfer::player_slot_to_boiler_fuel(&mut sim, boiler_id, 0),
        Err(BoilerError::InvalidFuel(iron_plate))
    );
    assert_eq!(sim.state_hash(), before_hash);
    assert_eq!(sim.power, power_before);

    sim.player_inventory = one_slot_inventory(&catalog, coal, 60);
    assert_eq!(
        crate::entity_transfer::player_slot_to_boiler_fuel(&mut sim, boiler_id, 0).unwrap(),
        TransferOutcome { moved_quantity: 50 }
    );
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 10);
    assert!(sim.power.networks.is_empty());
    assert!(sim.power.entity_statuses.is_empty());

    sim.tick();
    sim.player_inventory = one_slot_inventory(&catalog, coal, 99);
    sim.entities
        .boiler_state_mut(boiler_id)
        .unwrap()
        .energy
        .fuel_slot = test_slot(ItemStack::new(&catalog, coal, 10).unwrap());
    assert_eq!(
        crate::entity_transfer::boiler_fuel_to_player(&mut sim, boiler_id).unwrap(),
        TransferOutcome { moved_quantity: 1 }
    );
    assert_eq!(
        sim.entities
            .boiler_state(boiler_id)
            .unwrap()
            .energy
            .fuel_slot
            .stack()
            .unwrap()
            .count(),
        9
    );
    assert!(sim.power.networks.is_empty());
    assert!(sim.power.entity_statuses.is_empty());
}

#[test]
fn source_and_entity_errors_leave_the_entire_simulation_unchanged() {
    let mut sim = Simulation::new_test_world(123);
    let catalog = sim.world.prototypes.clone();
    let iron_plate = item_id(&catalog, "iron_plate");
    let chest_id = place_chest(&mut sim);
    sim.player_inventory = Inventory::with_slot_count(1);

    let before = sim.state_hash();
    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, chest_id, 0),
        Err(ContainerError::EmptySlot { slot_index: 0 })
    );
    assert_eq!(sim.state_hash(), before);

    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, chest_id, 1),
        Err(ContainerError::InvalidSlot { slot_index: 1 })
    );
    assert_eq!(sim.state_hash(), before);

    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 1);
    let before = sim.state_hash();
    assert_eq!(
        crate::entity_transfer::player_slot_to_furnace_input(&mut sim, chest_id, 0),
        Err(FurnaceError::NotFurnace(chest_id))
    );
    assert_eq!(sim.state_hash(), before);

    let missing_id = EntityId::new(u64::MAX);
    assert_eq!(
        crate::entity_transfer::player_slot_to_entity(&mut sim, missing_id, 0),
        Err(ContainerError::MissingEntity(missing_id))
    );
    assert_eq!(sim.state_hash(), before);
}
