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
    let stopped_state = crate::entity_access::boiler_state(&sim, boiler_id)
        .unwrap()
        .clone();
    assert_eq!(sim.power_summary().production_watts, 0);

    for _ in 0..120 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::boiler_state(&sim, boiler_id).unwrap(),
        &stopped_state
    );
    assert_eq!(sim.power_summary().production_watts, 0);
}

#[test]
fn boiler_clears_insufficient_residual_energy_without_fuel() {
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
    let state = sim
        .entities
        .boiler_state_mut(boiler_id)
        .expect("boiler should exist");
    state.energy.fuel_slot = None;
    state.energy.energy_remaining_joules = 1.0;

    sim.tick();

    let state = crate::entity_access::boiler_state(&sim, boiler_id).unwrap();
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
        .fuel_slot = Some(test_stack(iron_ore, 1));

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
    let assembler_id = crate::placement::place(
        &mut no_fuel,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
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
    crate::entity_mutation::remove(&mut no_water, pump_id)
        .expect("offshore pump should be removable");
    let assembler = entity_id_by_name(&no_water.world.prototypes, "assembling_machine");
    let assembler_id = crate::placement::place(
        &mut no_water,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
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
    crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pump,
            x: shore_x,
            y: shore_y,
            direction: Direction::North,
        },
    )
    .expect("offshore pump should place on shoreline");

    let away = first_buildable_offshore_pump_footprint_away_from_water(&sim, pump);
    assert!(matches!(
        crate::placement::validate(
            &sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: pump,
                x: away.0,
                y: away.1,
                direction: Direction::North
            }
        ),
        Err(BuildError::TileBlocked { .. })
    ));
}

#[test]
fn offshore_pump_produces_water_into_its_fluid_network() {
    let mut sim = Simulation::new_test_world(123);
    let pump = entity_id_by_name(&sim.world.prototypes, "offshore_pump");
    let water = fluid_id(&sim.world.prototypes, "water");
    let (x, y) = first_placeable_offshore_pump(&sim, pump);
    let pump_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pump,
            x,
            y,
            direction: Direction::North,
        },
    )
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

    let row = sim
        .fluid_statistics()
        .rows
        .into_iter()
        .find(|row| row.fluid_id == water)
        .expect("water production should be recorded");
    assert!(row.produced_last_minute > 0);
    assert_eq!(row.produced_last_minute, row.produced_total);
    assert_eq!(row.consumed_total, 0);
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

    let boiler = crate::entity_access::boiler_state(&sim, boiler_id).expect("boiler should exist");
    assert_eq!(boiler.energy.fuel_slot.map(|stack| stack.count()), Some(49));
    assert!(boiler.energy.energy_remaining_joules > 0.0);
    assert_eq!(
        sim.entities.fluid_boxes[&boiler_id][1].fluid_id,
        Some(steam)
    );
    assert!(sim.entities.fluid_boxes[&boiler_id][1].amount_milliunits > 0);

    let rows = sim.fluid_statistics().rows;
    let water = fluid_id(&sim.world.prototypes, "water");
    let water_row = rows
        .iter()
        .find(|row| row.fluid_id == water)
        .expect("water stats should exist");
    let steam_row = rows
        .iter()
        .find(|row| row.fluid_id == steam)
        .expect("steam stats should exist");
    assert!(water_row.consumed_total > 0);
    assert!(steam_row.produced_total > 0);
}

#[test]
fn boiler_does_not_consume_fuel_without_water_or_when_steam_output_is_full() {
    let mut no_water = Simulation::new_test_world(123);
    let boiler = entity_id_by_name(&no_water.world.prototypes, "boiler");
    let coal = item_id(&no_water.world.prototypes, "coal");
    let (x, y) = first_buildable_rect(&no_water.world, 2, 3);
    let boiler_id = crate::placement::place(
        &mut no_water,
        crate::placement::EntityPlacementRequest {
            prototype_id: boiler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("boiler should be placeable");
    no_water
        .entities
        .boiler_state_mut(boiler_id)
        .unwrap()
        .energy
        .fuel_slot = Some(test_stack(coal, 1));
    let before = crate::entity_access::boiler_state(&no_water, boiler_id)
        .unwrap()
        .clone();
    no_water.tick();
    assert_eq!(
        crate::entity_access::boiler_state(&no_water, boiler_id).unwrap(),
        &before
    );

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
    let before = crate::entity_access::boiler_state(&steam_full, boiler_id)
        .unwrap()
        .clone();

    steam_full.tick();

    assert_eq!(
        crate::entity_access::boiler_state(&steam_full, boiler_id).unwrap(),
        &before
    );
}

#[test]
fn steam_engine_consumes_steam_and_produces_electricity_for_demand() {
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

    let steam_row = sim
        .fluid_statistics()
        .rows
        .into_iter()
        .find(|row| row.fluid_id == steam)
        .expect("steam stats should exist");
    assert!(steam_row.consumed_total > 0);
}

#[test]
fn steam_engine_cannot_produce_without_steam() {
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
    let tank_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: tank,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("storage tank should be placeable");
    let pipe_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x: x + 3,
            y: y + 1,
            direction: Direction::North,
        },
    )
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
    assert!(sim.fluid_statistics().rows.is_empty());
}

#[test]
fn fluid_connection_directions_reports_joined_neighbors() {
    let mut sim = Simulation::new_test_world(123);
    let tank = entity_id_by_name(&sim.world.prototypes, "storage_tank");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let (x, y) = first_buildable_rect(&sim.world, 5, 3);
    let tank_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: tank,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("storage tank should be placeable");
    let first_pipe = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x: x + 3,
            y: y + 1,
            direction: Direction::North,
        },
    )
    .expect("pipe should connect to tank east port");
    let second_pipe = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x: x + 4,
            y: y + 1,
            direction: Direction::North,
        },
    )
    .expect("second pipe should be placeable");

    let directions = |entity_id| crate::entity_access::fluid_connection_directions(&sim, entity_id);
    let mask = |connected: [bool; 4]| {
        Direction::ALL
            .into_iter()
            .filter(|direction| connected[direction.index()])
            .collect::<Vec<_>>()
    };

    assert_eq!(
        mask(directions(first_pipe)),
        vec![Direction::East, Direction::West],
        "middle pipe should join the tank to its west and the pipe to its east"
    );
    assert_eq!(mask(directions(second_pipe)), vec![Direction::West]);
    assert!(
        directions(tank_id)[Direction::East.index()],
        "tank east port should join the adjacent pipe"
    );
    assert_eq!(mask(directions(EntityId::new(u64::MAX))), Vec::new());
}

#[test]
fn removing_pipe_splits_fluid_network_without_invalid_fluid_state() {
    let mut sim = Simulation::new_test_world(123);
    let tank = entity_id_by_name(&sim.world.prototypes, "storage_tank");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let water = fluid_id(&sim.world.prototypes, "water");
    let (x, y) = first_buildable_rect(&sim.world, 8, 3);
    let first_tank = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: tank,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("first tank should be placeable");
    let pipe_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x: x + 3,
            y: y + 1,
            direction: Direction::North,
        },
    )
    .expect("pipe should be placeable");
    let second_tank = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: tank,
            x: x + 4,
            y,
            direction: Direction::North,
        },
    )
    .expect("second tank should be placeable");
    set_fluid_box(&mut sim, first_tank, 0, water, 10_000_000);
    set_fluid_box(&mut sim, second_tank, 0, water, 5_000_000);
    sim.tick();
    let total_before = total_fluid_amount(&sim, water);
    let removed_pipe_amount = sim.entities.fluid_boxes[&pipe_id][0].amount_milliunits;

    crate::entity_mutation::remove(&mut sim, pipe_id).expect("pipe should be removable");
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
    let pipe_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("pipe should be placeable");
    let tank_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: tank,
            x: x + 1,
            y,
            direction: Direction::North,
        },
    )
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
fn filtered_empty_network_rejects_wrong_fluid_before_insert() {
    let mut catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let water = fluid_id(&catalog, "water");
    let steam = fluid_id(&catalog, "steam");
    let pipe = entity_id_by_name(&catalog, "pipe");
    let tank = entity_id_by_name(&catalog, "storage_tank");
    let zero_offset = catalog.entities[pipe.index()].fluid_boxes[0].connections[0].local_offset;
    catalog.entities[pipe.index()].fluid_boxes[0].filter = Some(water);
    catalog.entities[tank.index()].size.x = 1;
    catalog.entities[tank.index()].size.y = 1;
    catalog.entities[tank.index()].fluid_boxes[0].filter = None;
    catalog.entities[tank.index()].fluid_boxes[0].capacity_milliunits = 100_000;
    catalog.entities[tank.index()].fluid_boxes[0].connections =
        vec![factory_data::FluidConnectionPrototype {
            local_offset: zero_offset,
            side: factory_data::FluidConnectionSide::West,
        }];
    let mut sim = Simulation::new(123, catalog);
    let (x, y) = first_buildable_rect(&sim.world, 2, 1);
    let pipe_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("pipe should be placeable");
    let tank_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: tank,
            x: x + 1,
            y,
            direction: Direction::North,
        },
    )
    .expect("tank should be placeable");
    sim.ensure_fluid_network_topology();
    let network_id = sim
        .fluid_network_id_for_box_key(FluidBoxKey {
            entity_id: tank_id,
            box_index: 0,
        })
        .expect("tank should be in a fluid network");

    assert_eq!(
        sim.fluid_network_available_capacity_for_fluid(network_id, steam),
        0
    );
    assert_eq!(sim.add_fluid_to_network(network_id, steam, 10_000), 0);
    assert_eq!(
        sim.entities.fluid_boxes[&pipe_id][0],
        FluidBoxState::default()
    );
    assert_eq!(
        sim.entities.fluid_boxes[&tank_id][0],
        FluidBoxState::default()
    );
    assert!(sim.fluid_network_available_capacity_for_fluid(network_id, water) > 0);
    assert_eq!(sim.add_fluid_to_network(network_id, water, 10_000), 10_000);
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

#[test]
fn fluid_topology_rebuilds_once_across_repeated_ticks_without_entity_changes() {
    let mut sim = Simulation::new_test_world(123);
    place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));

    sim.tick();
    let rebuilds_after_first_tick = sim.fluid_topology_rebuild_count();
    assert_eq!(rebuilds_after_first_tick, 1);

    for _ in 0..10 {
        sim.tick();
    }

    assert_eq!(
        sim.fluid_topology_rebuild_count(),
        rebuilds_after_first_tick
    );
}

#[test]
fn fluid_topology_rebuilds_after_fluid_entity_placement_or_removal() {
    let mut sim = Simulation::new_test_world(123);
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let (x, y) = first_buildable_rect(&sim.world, 1, 1);

    sim.tick();
    let rebuilds_after_first_tick = sim.fluid_topology_rebuild_count();

    let pipe_id = crate::placement::place(
        &mut sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: pipe,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("pipe should be placeable");
    sim.tick();

    let rebuilds_after_placement = sim.fluid_topology_rebuild_count();
    assert_eq!(rebuilds_after_placement, rebuilds_after_first_tick + 1);

    crate::entity_mutation::remove(&mut sim, pipe_id).expect("pipe should be removable");
    sim.tick();

    assert_eq!(
        sim.fluid_topology_rebuild_count(),
        rebuilds_after_placement + 1
    );
}

#[test]
fn profiled_tick_preserves_hash_against_plain_tick_after_fluid_refactor() {
    let mut profiled = Simulation::new_test_world(123);
    let mut ticked = Simulation::new_test_world(123);
    place_powered_fixture_origin_with_boiler(&mut profiled, 3, 3, (3, 1));
    place_powered_fixture_origin_with_boiler(&mut ticked, 3, 3, (3, 1));

    for _ in 0..16 {
        profiled.profiled_tick();
        ticked.tick();
        assert_eq!(profiled.state_hash(), ticked.state_hash());
    }
}
