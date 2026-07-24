use super::super::*;
use super::support::*;

#[test]
fn module_stack_transfers_to_an_ordinary_chest() {
    let mut sim = Simulation::new_test_world(141);
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    let speed = item_id(&sim.world.prototypes, "speed_module_1");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let chest_id = crate::placement::place(
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
    set_inventory_slot(&mut sim.player_inventory, 0, speed, 12);

    let outcome = crate::entity_transfer::transfer_container_slot(
        &mut sim,
        chest_id,
        InventoryPanel::Player,
        0,
    )
    .expect("modules should remain ordinary items in chest inventories");

    assert_eq!(outcome.moved_quantity, 12);
    assert_eq!(sim.player_inventory.count(speed), 0);
    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .unwrap()
            .count(speed),
        12
    );
}

#[test]
fn player_transfer_installs_one_module_per_slot_and_removes_it() {
    let mut sim = Simulation::new_test_world(141);
    let assembler_id = place_assembling_machine(&mut sim);
    let catalog = sim.world.prototypes.clone();
    let speed = item_id(&catalog, "speed_module_1");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, speed, 3);

    let outcome = crate::entity_transfer::transfer_container_slot(
        &mut sim,
        assembler_id,
        InventoryPanel::Player,
        0,
    )
    .expect("speed modules should install");
    assert_eq!(outcome.moved_quantity, 2);
    assert_eq!(sim.player_inventory.slot(0).unwrap().count(), 1);
    let slots = crate::entity_access::module_slots(&sim, assembler_id).unwrap();
    assert_eq!(slots.slot(0).unwrap().count(), 1);
    assert_eq!(slots.slot(1).unwrap().count(), 1);

    let effects = crate::entity_access::resolved_module_effects(&sim, assembler_id).unwrap();
    assert_eq!(effects.speed_multiplier_permyriad(), 14_000);
    assert_eq!(effects.energy_multiplier_permyriad(), 20_000);

    crate::entity_transfer::transfer_container_slot(
        &mut sim,
        assembler_id,
        InventoryPanel::Modules,
        0,
    )
    .expect("installed module should return to player");
    assert!(
        crate::entity_access::module_slots(&sim, assembler_id)
            .unwrap()
            .slot(0)
            .is_none()
    );
    assert_eq!(sim.player_inventory.count(speed), 2);
}

#[test]
fn speed_modules_update_cached_recipe_duration_without_resetting_productivity() {
    let mut sim = Simulation::new_test_world(141);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    sim.select_assembler_recipe(assembler_id, recipe).unwrap();
    sim.entities
        .assembler_state_mut(assembler_id)
        .unwrap()
        .modules
        .productivity_progress_permyriad = 725;

    let catalog = sim.world.prototypes.clone();
    let speed = item_id(&catalog, "speed_module_1");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, speed, 2);
    crate::entity_transfer::transfer_container_slot(
        &mut sim,
        assembler_id,
        InventoryPanel::Player,
        0,
    )
    .unwrap();

    let state = crate::entity_access::assembler_state(&sim, assembler_id).unwrap();
    assert_eq!(state.crafting_required_ticks, 43);
    assert_eq!(state.modules.productivity_progress_permyriad, 725);
}

#[test]
fn fixed_point_effect_floors_and_pollution_formula_are_exact() {
    let effects = ResolvedModuleEffects {
        speed_delta_permyriad: -50_000,
        productivity_permyriad: 1_000,
        energy_delta_permyriad: -8_000,
        pollution_delta_permyriad: 5_000,
    };
    assert_eq!(effects.speed_multiplier_permyriad(), 2_000);
    assert_eq!(effects.energy_multiplier_permyriad(), 2_000);
    assert_eq!(effects.explicit_pollution_multiplier_permyriad(), 15_000);
    assert_eq!(effects.pollution_multiplier_permyriad(), 3_000);
    assert_eq!(effects.productivity_permyriad(), 1_000);
}

#[test]
fn furnace_productivity_survives_a_same_recipe_input_gap() {
    let mut sim = Simulation::new_test_world(141);
    let furnace_id = place_named_furnace(&mut sim, "electric_furnace");
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");

    sim.entities
        .furnace_state_mut(furnace_id)
        .unwrap()
        .input_slot = test_slot(test_stack(iron_ore, 1));
    sim.tick();

    let state = sim.entities.furnace_state_mut(furnace_id).unwrap();
    let active_recipe = state.active_recipe.expect("input should select a recipe");
    state.modules.productivity_progress_permyriad = 975;
    state.input_slot = ItemSlot::default();

    sim.tick();
    let state = sim.entities.furnace_state_mut(furnace_id).unwrap();
    assert_eq!(state.active_recipe, Some(active_recipe));
    assert_eq!(state.modules.productivity_progress_permyriad, 975);
    state.input_slot = test_slot(test_stack(iron_ore, 1));

    sim.tick();
    let state = crate::entity_access::furnace_state(&sim, furnace_id).unwrap();
    assert_eq!(state.active_recipe, Some(active_recipe));
    assert_eq!(state.modules.productivity_progress_permyriad, 975);
}

#[test]
fn productivity_modules_are_intentionally_transmitted_by_beacons() {
    let mut sim = Simulation::new_test_world(141);
    assert_eq!(sim.world.max_beacon_effect_radius_tiles, 3);
    let beacon = entity_id_by_name(&sim.world.prototypes, "beacon");
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let productivity = item_id(&sim.world.prototypes, "productivity_module_1");
    let (x, y) = first_buildable_rect(&sim.world, 6, 3);
    let beacon_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: beacon,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("beacon should be placeable");
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x: x + 3,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable in beacon range");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, productivity, 1);

    crate::entity_transfer::transfer_container_slot(&mut sim, beacon_id, InventoryPanel::Player, 0)
        .expect("issue #141 allows productivity modules in beacons");

    assert_eq!(
        crate::entity_access::resolved_module_effects(&sim, assembler_id)
            .unwrap()
            .productivity_permyriad(),
        200
    );
}

#[test]
fn boundary_beacons_stack_once_each_and_removal_refreshes_nearby_machine() {
    let mut sim = Simulation::new_test_world(141);
    let beacon = entity_id_by_name(&sim.world.prototypes, "beacon");
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&sim.world, 8, 6);

    let first_beacon = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: beacon,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("first beacon should be placeable");
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x: x + 5,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable on the beacon boundary");
    let second_beacon = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: beacon,
            x: x + 5,
            y: y + 3,
            direction: Direction::North,
        },
    )
    .expect("second beacon should be placeable");

    let catalog = sim.world.prototypes.clone();
    let speed = item_id(&catalog, "speed_module_1");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, speed, 1);
    crate::entity_transfer::transfer_container_slot(
        &mut sim,
        first_beacon,
        InventoryPanel::Player,
        0,
    )
    .expect("first beacon should accept a module");
    assert_eq!(
        crate::entity_access::resolved_module_effects(&sim, assembler_id)
            .unwrap()
            .speed_multiplier_permyriad(),
        11_000
    );

    set_inventory_slot(&mut sim.player_inventory, 0, speed, 1);
    crate::entity_transfer::transfer_container_slot(
        &mut sim,
        second_beacon,
        InventoryPanel::Player,
        0,
    )
    .expect("second beacon should accept a module");
    assert_eq!(
        crate::entity_access::resolved_module_effects(&sim, assembler_id)
            .unwrap()
            .speed_multiplier_permyriad(),
        12_000
    );

    crate::entity_mutation::remove(&mut sim, first_beacon);
    assert_eq!(
        crate::entity_access::resolved_module_effects(&sim, assembler_id)
            .unwrap()
            .speed_multiplier_permyriad(),
        11_000
    );
    crate::entity_mutation::remove(&mut sim, second_beacon);
    assert_eq!(
        crate::entity_access::resolved_module_effects(&sim, assembler_id)
            .unwrap()
            .speed_multiplier_permyriad(),
        10_000
    );
}
