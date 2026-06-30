use super::super::*;
use super::support::*;

#[test]
fn boiler_fills_steam_buffer_without_demand_then_stops_when_full() {
    let mut sim = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    let steam = fluid_id(&sim.world.prototypes, "steam");

    let mut reached_capacity = false;
    for _ in 0..1_000 {
        sim.tick();
        let Some(steam_network) = sim
            .fluid_networks()
            .iter()
            .find(|network| network.fluid_id == Some(steam))
        else {
            continue;
        };
        if steam_network.total_milliunits == steam_network.capacity_milliunits {
            reached_capacity = true;
            break;
        }
    }

    assert!(reached_capacity);
    let stopped_state = sim.boiler_state(boiler_id).unwrap().clone();
    assert_eq!(sim.power_summary().production_watts, 0);

    for _ in 0..120 {
        sim.tick();
    }

    assert_eq!(sim.boiler_state(boiler_id).unwrap(), &stopped_state);
    assert_eq!(sim.power_summary().production_watts, 0);
}

#[test]
fn boiler_clears_insufficient_residual_energy_without_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);
    let state = sim
        .entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist");
    state.energy.fuel_slot = None;
    state.energy.energy_remaining_joules = 1.0;

    sim.tick();

    let state = sim.boiler_state(boiler_id).unwrap();
    assert_eq!(state.energy.fuel_slot, None);
    assert_eq!(state.energy.energy_remaining_joules, 0.0);
}

#[test]
fn boiler_validation_rejects_non_fuel_in_fuel_slot() {
    let mut sim = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    sim.tick();
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    sim.entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist")
        .energy
        .fuel_slot = Some(ItemStack {
        item_id: iron_ore,
        count: 1,
    });

    assert_eq!(
        sim.validate(),
        Err(SimValidationError::InvalidMachineItem {
            entity_id: boiler_id,
            item_id: iron_ore,
        })
    );
}

#[test]
fn boiler_with_no_water_or_no_fuel_produces_no_steam_power() {
    let mut no_fuel = Simulation::new_test_world(123);
    let (x, y, boiler_id) = place_powered_fixture_origin_with_boiler(&mut no_fuel, 3, 3, (3, 1));
    no_fuel
        .entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist")
        .energy
        .fuel_slot = None;
    let assembler = entity_id_by_name(&no_fuel.world.prototypes, "assembling_machine");
    let assembler_id = no_fuel
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut no_fuel, assembler_id);
    no_fuel.tick();
    assert_eq!(no_fuel.power_summary().available_production_watts, 0);

    let mut no_water = Simulation::new_test_world(123);
    let (x, y, _) = place_powered_fixture_origin_with_boiler(&mut no_water, 3, 3, (3, 1));
    let pump_id = *no_water
        .entities
        .offshore_pumps
        .keys()
        .next()
        .expect("fixture should place an offshore pump");
    no_water
        .remove_entity(pump_id)
        .expect("offshore pump should be removable");
    let assembler = entity_id_by_name(&no_water.world.prototypes, "assembling_machine");
    let assembler_id = no_water
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut no_water, assembler_id);
    no_water.tick();
    assert_eq!(no_water.power_summary().available_production_watts, 0);
}

#[test]
fn offshore_pump_placement_succeeds_on_shoreline_and_fails_away_from_water() {
    let mut sim = Simulation::new_test_world(123);
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let (shore_x, shore_y) = first_placeable_offshore_pump(&sim, pump);
    sim.place_entity(pump, shore_x, shore_y, Direction::North)
        .expect("offshore pump should place on shoreline");

    let away = first_buildable_offshore_pump_footprint_away_from_water(&sim, pump);
    assert!(matches!(
        sim.can_place_entity(pump, away.0, away.1, Direction::North),
        Err(BuildError::TileBlocked { .. })
    ));
}

#[test]
fn offshore_pump_produces_water_into_its_fluid_network() {
    let mut sim = Simulation::new_test_world(123);
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let water = fluid_id(&sim.world.prototypes, "water");
    let (x, y) = first_placeable_offshore_pump(&sim, pump);
    let pump_id = sim
        .place_entity(pump, x, y, Direction::North)
        .expect("offshore pump should place on shoreline");

    sim.tick();

    let pump_box = &sim.entities.fluid_boxes[&pump_id][0];
    assert_eq!(pump_box.fluid_id, Some(water));
    assert!(pump_box.amount_milliunits > 0);
    assert!(
        sim.fluid_networks()
            .iter()
            .any(|network| network.fluid_id == Some(water) && network.total_milliunits > 0)
    );
}

#[test]
fn pipe_between_offshore_pump_and_boiler_moves_water() {
    let mut sim = Simulation::new_test_world(123);
    let water = fluid_id(&sim.world.prototypes, "water");
    let (_pump_id, pipe_id, boiler_id) = place_pump_pipe_boiler_fixture(&mut sim);

    sim.tick();

    assert_eq!(sim.entities.fluid_boxes[&pipe_id][0].fluid_id, Some(water));
    assert!(sim.entities.fluid_boxes[&pipe_id][0].amount_milliunits > 0);
    assert_eq!(
        sim.entities.fluid_boxes[&boiler_id][0].fluid_id,
        Some(water)
    );
    assert!(sim.entities.fluid_boxes[&boiler_id][0].amount_milliunits > 0);
}

#[test]
fn boiler_consumes_water_and_fuel_and_outputs_steam() {
    let mut sim = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    let steam = fluid_id(&sim.world.prototypes, "steam");

    sim.tick();

    let boiler = sim.boiler_state(boiler_id).expect("boiler should exist");
    assert_eq!(boiler.energy.fuel_slot.map(|stack| stack.count), Some(49));
    assert!(boiler.energy.energy_remaining_joules > 0.0);
    assert_eq!(
        sim.entities.fluid_boxes[&boiler_id][1].fluid_id,
        Some(steam)
    );
    assert!(sim.entities.fluid_boxes[&boiler_id][1].amount_milliunits > 0);
}

#[test]
fn boiler_does_not_consume_fuel_without_water_or_when_steam_output_is_full() {
    let mut no_water = Simulation::new_test_world(123);
    let boiler = entity_id_by_name(&no_water.world.prototypes, "boiler");
    let coal = item_id(&no_water.world.prototypes, "coal");
    let (x, y) = first_buildable_rect(&no_water.world, 2, 3);
    let boiler_id = no_water
        .place_entity(boiler, x, y, Direction::North)
        .expect("boiler should be placeable");
    no_water
        .entities
        .boiler_state_mut(boiler_id)
        .unwrap()
        .energy
        .fuel_slot = Some(ItemStack {
        item_id: coal,
        count: 1,
    });
    let before = no_water.boiler_state(boiler_id).unwrap().clone();
    no_water.tick();
    assert_eq!(no_water.boiler_state(boiler_id).unwrap(), &before);

    let mut steam_full = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut steam_full, 1, 1, (1, 2));
    let steam = fluid_id(&steam_full.world.prototypes, "steam");
    let engine_id = *steam_full
        .entities
        .steam_engines
        .keys()
        .next()
        .expect("fixture should place a steam engine");
    set_fluid_box(&mut steam_full, boiler_id, 1, steam, 100_000);
    set_fluid_box(&mut steam_full, engine_id, 0, steam, 100_000);
    let before = steam_full.boiler_state(boiler_id).unwrap().clone();

    steam_full.tick();

    assert_eq!(steam_full.boiler_state(boiler_id).unwrap(), &before);
}

#[test]
fn steam_engine_consumes_steam_and_produces_electricity_for_demand() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);
    for state in sim.entities.boilers.values_mut() {
        state.energy.fuel_slot = None;
        state.energy.energy_remaining_joules = 0.0;
    }
    let steam = fluid_id(&sim.world.prototypes, "steam");
    let engine_id = *sim
        .entities
        .steam_engines
        .keys()
        .next()
        .expect("fixture should place a steam engine");
    set_fluid_box(&mut sim, boiler_id, 1, steam, 100_000);

    sim.tick();

    assert_eq!(sim.power_summary().production_watts, 77_500);
    assert!(sim.entities.fluid_boxes[&engine_id][0].amount_milliunits > 0);
    assert!(sim.entities.fluid_boxes[&engine_id][0].amount_milliunits < 50_000);
    assert!(total_fluid_amount(&sim, steam) < 100_000);
}

#[test]
fn steam_engine_cannot_produce_without_steam() {
    let mut sim = Simulation::new_test_world(123);
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let assembler_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");
    add_assembler_gear_job(&mut sim, assembler_id);
    for state in sim.entities.boilers.values_mut() {
        state.energy.fuel_slot = None;
        state.energy.energy_remaining_joules = 0.0;
    }

    sim.tick();

    assert_eq!(sim.power_summary().available_production_watts, 0);
    assert_eq!(sim.power_summary().production_watts, 0);
}

#[test]
fn storage_tank_equalizes_with_connected_pipe_by_fill_percentage() {
    let mut sim = Simulation::new_test_world(123);
    let tank = entity_id_by_name(&sim.world.prototypes, "storage_tank");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let water = fluid_id(&sim.world.prototypes, "water");
    let (x, y) = first_buildable_rect(&sim.world, 4, 3);
    let tank_id = sim
        .place_entity(tank, x, y, Direction::North)
        .expect("storage tank should be placeable");
    let pipe_id = sim
        .place_entity(pipe, x + 3, y + 1, Direction::North)
        .expect("pipe should connect to tank east port");
    set_fluid_box(&mut sim, tank_id, 0, water, 12_550_000);

    sim.tick();

    assert_eq!(
        sim.entities.fluid_boxes[&tank_id][0].amount_milliunits,
        12_500_000
    );
    assert_eq!(
        sim.entities.fluid_boxes[&pipe_id][0].amount_milliunits,
        50_000
    );
}

#[test]
fn removing_pipe_splits_fluid_network_without_invalid_fluid_state() {
    let mut sim = Simulation::new_test_world(123);
    let tank = entity_id_by_name(&sim.world.prototypes, "storage_tank");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let water = fluid_id(&sim.world.prototypes, "water");
    let (x, y) = first_buildable_rect(&sim.world, 8, 3);
    let first_tank = sim
        .place_entity(tank, x, y, Direction::North)
        .expect("first tank should be placeable");
    let pipe_id = sim
        .place_entity(pipe, x + 3, y + 1, Direction::North)
        .expect("pipe should be placeable");
    let second_tank = sim
        .place_entity(tank, x + 4, y, Direction::North)
        .expect("second tank should be placeable");
    set_fluid_box(&mut sim, first_tank, 0, water, 10_000_000);
    set_fluid_box(&mut sim, second_tank, 0, water, 5_000_000);
    sim.tick();
    let total_before = total_fluid_amount(&sim, water);
    let removed_pipe_amount = sim.entities.fluid_boxes[&pipe_id][0].amount_milliunits;

    sim.remove_entity(pipe_id)
        .expect("pipe should be removable");
    sim.tick();

    assert_eq!(
        total_fluid_amount(&sim, water),
        total_before - removed_pipe_amount
    );
    assert_eq!(
        sim.fluid_networks()
            .iter()
            .filter(|network| network.fluid_id == Some(water))
            .count(),
        2
    );
    sim.validate()
        .expect("split fluid networks should validate");
}

#[test]
fn incompatible_water_and_steam_network_is_blocked_and_does_not_mix() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let water = fluid_id(&catalog, "water");
    let steam = fluid_id(&catalog, "steam");
    let pipe = entity_id_by_name(&catalog, "pipe");
    let tank = entity_id_by_name(&catalog, "storage_tank");
    let zero_offset = catalog.entities[pipe.index()].fluid_boxes[0].connections[0].local_offset;
    catalog.entities[pipe.index()].fluid_boxes[0].filter = Some(water);
    catalog.entities[tank.index()].size.x = 1;
    catalog.entities[tank.index()].size.y = 1;
    catalog.entities[tank.index()].fluid_boxes[0].filter = Some(steam);
    catalog.entities[tank.index()].fluid_boxes[0].capacity_milliunits = 100_000;
    catalog.entities[tank.index()].fluid_boxes[0].connections =
        vec![factory_data::FluidConnectionPrototype {
            local_offset: zero_offset,
            side: factory_data::FluidConnectionSide::West,
        }];
    let mut sim = Simulation::new(123, catalog);
    let (x, y) = first_buildable_rect(&sim.world, 2, 1);
    let pipe_id = sim
        .place_entity(pipe, x, y, Direction::North)
        .expect("pipe should be placeable");
    let tank_id = sim
        .place_entity(tank, x + 1, y, Direction::North)
        .expect("tank should be placeable");
    set_fluid_box(&mut sim, pipe_id, 0, water, 10_000);
    set_fluid_box(&mut sim, tank_id, 0, steam, 10_000);

    sim.tick();

    let network = sim
        .fluid_networks()
        .iter()
        .find(|network| network.box_count == 2)
        .expect("conflicting boxes should be in one network");
    assert!(network.blocked);
    assert_eq!(sim.entities.fluid_boxes[&pipe_id][0].fluid_id, Some(water));
    assert_eq!(sim.entities.fluid_boxes[&tank_id][0].fluid_id, Some(steam));
    assert_eq!(
        sim.entities.fluid_boxes[&pipe_id][0].amount_milliunits,
        10_000
    );
    assert_eq!(
        sim.entities.fluid_boxes[&tank_id][0].amount_milliunits,
        10_000
    );
}

#[test]
fn save_load_preserves_fluid_boxes_networks_and_state_hash() {
    let mut sim = Simulation::new_test_world(123);
    let (_pump_id, pipe_id, _boiler_id) = place_pump_pipe_boiler_fixture(&mut sim);
    for _ in 0..5 {
        sim.tick();
    }
    let before_hash = sim.state_hash();
    let before_box = sim.entities.fluid_boxes[&pipe_id].clone();
    let before_networks = sim.fluid_networks().to_vec();

    let bytes = save_to_bytes(&sim).expect("fluid sim should save");
    let loaded = load_from_bytes(&bytes).expect("fluid sim should load");

    assert_eq!(loaded.state_hash(), before_hash);
    assert_eq!(loaded.entities.fluid_boxes[&pipe_id], before_box);
    assert_eq!(loaded.fluid_networks(), before_networks.as_slice());
}
