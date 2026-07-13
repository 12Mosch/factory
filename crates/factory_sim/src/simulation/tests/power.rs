use super::super::*;
use super::support::*;

#[test]
fn unpowered_assembler_does_not_craft() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = first_buildable_rect(&sim.world, 3, 3);
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable");
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 2);
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..90 {
        sim.tick();
    }

    let state = crate::entity_access::assembler_state(&sim, assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.input_inventory.count(iron_plate), 2);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(state.crafting_progress_ticks, 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn powered_assembler_crafts() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 2);
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..60 {
        sim.tick();
    }

    let state = crate::entity_access::assembler_state(&sim, assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);

    let history = sim.power_statistics();
    assert!(history.samples.iter().any(|sample| {
        sample.production_watts > 0
            && sample.consumption_watts > 0
            && sample.satisfaction_permyriad > 0
    }));
}

#[test]
fn power_history_drops_samples_older_than_one_minute() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);

    for _ in 0..(ITEM_STATISTICS_WINDOW_TICKS + 5) {
        sim.tick();
    }

    let history = sim.power_statistics();
    assert!(!history.samples.is_empty());
    assert!(history.samples.iter().all(|sample| {
        sample.tick.saturating_add(ITEM_STATISTICS_WINDOW_TICKS) > sim.tick_count()
    }));
    assert!(history.samples.iter().all(|sample| sample.tick > 5));
}

#[test]
fn insufficient_power_slows_machine_progress() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let assembler = entity_id_by_name(&catalog, "assembling_machine");
    catalog.entities[assembler.index()]
        .electric_energy_source
        .as_mut()
        .expect("assembler should have electric energy source")
        .energy_usage_watts = 1_797_500;
    let mut sim = Simulation::new(123, catalog);
    let assembler_id = place_assembling_machine(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 2);
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..60 {
        sim.tick();
    }

    let state = crate::entity_access::assembler_state(&sim, assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.crafting_progress_ticks, 30);
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        5_000
    );

    for _ in 0..60 {
        sim.tick();
    }

    let state = crate::entity_access::assembler_state(&sim, assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 1);
}

#[test]
fn disconnected_networks_do_not_share_power() {
    let mut sim = Simulation::new_test_world(123);
    let _ = place_powered_fixture_origin(&mut sim, 1, 1, (1, 2));
    let assembler_id = place_disconnected_assembler_network(&mut sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let iron_gear_wheel = item_id(&sim.world.prototypes, "iron_gear_wheel");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 2);
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");

    for _ in 0..90 {
        sim.tick();
    }

    let state = crate::entity_access::assembler_state(&sim, assembler_id)
        .expect("assembler should expose state");
    assert_eq!(state.output_inventory.count(iron_gear_wheel), 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        0
    );
    assert!(sim.power_summary().network_count >= 2);
}

#[test]
fn small_pole_coverage_connects_nearby_machine_and_wire_reach_connects_networks() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable");

    sim.tick();

    let status = sim
        .entity_power_status(assembler_id)
        .expect("assembler should report power status");
    assert_eq!(status.network_id, Some(0));
    assert_eq!(sim.power_networks().len(), 1);
    assert_eq!(sim.power_networks()[0].pole_count, 2);
}

#[test]
fn pole_networks_outside_reach_do_not_connect() {
    let mut sim = Simulation::new_test_world(123);
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pole,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("first pole should be placeable");

    for (candidate_x, candidate_y) in all_tile_coords(&sim.world) {
        if !poles_within_small_pole_reach((x, y), (candidate_x, candidate_y))
            && crate::placement::validate(
                &sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: pole,
                    x: candidate_x,
                    y: candidate_y,
                    direction: Direction::North,
                },
            )
            .is_ok()
        {
            crate::placement::place(
                &mut sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: pole,
                    x: candidate_x,
                    y: candidate_y,
                    direction: Direction::North,
                },
            )
            .expect("second pole should be placeable");
            sim.tick();
            assert_eq!(sim.power_summary().network_count, 2);
            return;
        }
    }

    panic!("expected a second pole location outside wire reach");
}

#[test]
fn steam_engine_produces_only_with_connected_pole_and_adjacent_fueled_boiler() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();

    let summary = sim.power_summary();
    assert_eq!(summary.available_production_watts, 79_200);
    assert_eq!(summary.production_watts, 77_500);
    assert_eq!(summary.consumption_watts, 77_500);
    assert_eq!(summary.satisfaction_permyriad, 10_000);
}

#[test]
fn electricity_generated_milestone_fires_without_any_connected_consumer() {
    let mut sim = Simulation::new_test_world(123);
    place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));

    for _ in 0..60 {
        sim.tick();
    }

    let summary = sim.power_summary();
    assert_eq!(summary.consumption_watts, 0);
    assert_eq!(summary.production_watts, 0);
    assert_eq!(summary.available_production_watts, 0);
    assert!(sim.onboarding_progress().electricity_generated);
}

#[test]
fn inserter_does_not_move_without_electricity() {
    let mut sim = Simulation::new_test_world(123);
    let (chest_id, inserter_id, furnace_id) = place_unpowered_chest_inserter_furnace_line(&mut sim);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should expose inventory"),
        0,
        iron_ore,
        1,
    );

    for _ in 0..inserter_cycle_tick_budget(&sim, inserter_id) * 2 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .unwrap()
            .count(iron_ore),
        1
    );
    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .unwrap()
            .input_slot,
        None
    );
    assert_eq!(
        sim.entity_power_status(inserter_id)
            .expect("inserter should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn lab_does_not_research_without_electricity() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let logistics = technology_id(&sim.world.prototypes, "logistics");
    let science_pack = item_id(&sim.world.prototypes, "automation_science_pack");
    let (x, y) = first_buildable_rect(&sim.world, 3, 3);
    let lab_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: lab,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("lab should be placeable");
    sim.select_research(logistics)
        .expect("logistics should be selectable");
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, lab_id).expect("lab should expose inventory"),
        0,
        science_pack,
        10,
    );

    for _ in 0..1_200 {
        sim.tick();
    }

    assert_eq!(sim.technology_progress(logistics), Some(0));
    assert_eq!(
        sim.entity_power_status(lab_id)
            .expect("lab should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn power_summary_reports_production_consumption_and_satisfaction() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();

    assert_eq!(
        sim.power_summary(),
        PowerSummary {
            production_watts: 77_500,
            available_production_watts: 79_200,
            consumption_watts: 77_500,
            satisfaction_permyriad: 10_000,
            network_count: 1,
        }
    );
}

#[test]
fn initial_tick_builds_power_topology_once_and_preserves_power_summary() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);

    assert_eq!(sim.power_topology_rebuild_count(), 0);
    sim.tick();

    assert_eq!(sim.power_topology_rebuild_count(), 1);
    assert_eq!(
        sim.power_summary(),
        PowerSummary {
            production_watts: 77_500,
            available_production_watts: 79_200,
            consumption_watts: 77_500,
            satisfaction_permyriad: 10_000,
            network_count: 1,
        }
    );
}

#[test]
fn repeated_ticks_without_topology_edits_do_not_rebuild_power_topology() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();
    let rebuilds_after_initial_tick = sim.power_topology_rebuild_count();
    for _ in 0..10 {
        sim.tick();
    }

    assert_eq!(
        sim.power_topology_rebuild_count(),
        rebuilds_after_initial_tick
    );
}

#[test]
fn placing_electric_pole_dirties_and_rebuilds_power_topology() {
    let mut sim = Simulation::new_test_world(123);
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);

    sim.tick();
    let rebuilds_after_initial_tick = sim.power_topology_rebuild_count();
    crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pole,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("pole should be placeable");
    sim.tick();

    assert_eq!(
        sim.power_topology_rebuild_count(),
        rebuilds_after_initial_tick + 1
    );
}

#[test]
fn removing_electric_pole_dirties_and_rebuilds_power_topology() {
    let mut sim = Simulation::new_test_world(123);
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);
    let pole_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pole,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("pole should be placeable");

    sim.tick();
    let rebuilds_after_initial_tick = sim.power_topology_rebuild_count();
    crate::entity_mutation::remove(&mut sim, pole_id).expect("pole should be removable");
    sim.tick();

    assert_eq!(
        sim.power_topology_rebuild_count(),
        rebuilds_after_initial_tick + 1
    );
}

#[test]
fn placing_and_removing_electric_consumer_updates_coverage_and_power_status() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));

    sim.tick();
    let rebuilds_after_fixture = sim.power_topology_rebuild_count();
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("covered assembler should be placeable");
    sim.tick();

    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .network_id,
        Some(0)
    );
    assert_eq!(
        sim.power_topology_rebuild_count(),
        rebuilds_after_fixture + 1
    );

    crate::entity_mutation::remove(&mut sim, assembler_id).expect("assembler should be removable");
    sim.tick();

    assert!(sim.entity_power_status(assembler_id).is_none());
    assert_eq!(
        sim.power_topology_rebuild_count(),
        rebuilds_after_fixture + 2
    );
}

#[test]
fn boiler_fuel_and_fluid_changes_update_power_without_rebuilding_topology() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);

    sim.tick();
    assert!(sim.power_summary().available_production_watts > 0);
    let rebuilds_after_initial_tick = sim.power_topology_rebuild_count();

    {
        let boiler = sim
            .entities
            .boiler_state_mut(boiler_id)
            .expect("fixture boiler should expose state");
        boiler.energy.fuel_slot = None;
        boiler.energy.energy_remaining_joules = 0.0;
    }
    for boxes in sim.entities.fluid_boxes.values_mut() {
        for fluid_box in boxes {
            fluid_box.fluid_id = None;
            fluid_box.amount_milliunits = 0;
        }
    }
    sim.invalidate_fluid_state();
    sim.invalidate_power_dynamic_state();
    sim.tick();

    assert_eq!(
        sim.power_topology_rebuild_count(),
        rebuilds_after_initial_tick
    );
    assert_eq!(sim.power_summary().available_production_watts, 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report power status")
            .satisfaction_permyriad,
        0
    );
}

#[test]
fn save_load_preserves_state_hash_after_electricity_entities_exist() {
    let mut sim = Simulation::new_test_world(123);
    let assembler_id = place_assembling_machine(&mut sim);
    add_assembler_gear_job(&mut sim, assembler_id);
    for _ in 0..17 {
        sim.tick();
    }

    let before = sim.state_hash();
    let bytes = save_to_bytes(&sim).expect("electricity sim should save");
    let loaded = load_from_bytes(&bytes).expect("electricity sim should load");

    assert_eq!(before, loaded.state_hash());
}
