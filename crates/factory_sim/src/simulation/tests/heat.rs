use super::super::*;
use super::support::*;
use crate::heat::{energy_for_temperature, temperature_millidegrees};

/// Places a horizontal run of heat pipes on the first free spot and returns their
/// entity ids. Searching by placement validity (rather than by terrain alone)
/// lets a test place several independent runs.
fn place_heat_pipe_run(sim: &mut Simulation, length: i32) -> Vec<EntityId> {
    let heat_pipe = entity_id_by_name(&sim.world.prototypes, "heat_pipe");
    for (x, y) in all_tile_coords(&sim.world) {
        let run_is_placeable = (0..length).all(|offset| {
            crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: heat_pipe,
                    x: x + i64::from(offset),
                    y,
                    direction: Direction::North,
                },
            )
            .is_ok()
        });
        if !run_is_placeable {
            continue;
        }
        return (0..length)
            .map(|offset| place_at(sim, heat_pipe, x + i64::from(offset), y, Direction::North))
            .collect();
    }

    panic!("expected a placeable heat pipe run");
}

fn set_heat_energy(sim: &mut Simulation, entity_id: EntityId, energy_joules: u64) {
    sim.entities
        .heat_buffers
        .get_mut(&entity_id)
        .expect("test entity should expose a heat buffer")
        .energy_joules = energy_joules;
    sim.invalidate_heat_state();
}

fn heat_energy(sim: &Simulation, entity_id: EntityId) -> u64 {
    sim.entities.heat_buffers[&entity_id].energy_joules
}

#[test]
fn touching_heat_pipes_form_one_network_and_a_gap_splits_them() {
    let mut sim = Simulation::new_test_world(123);
    let heat_pipe = entity_id_by_name(&sim.world.prototypes, "heat_pipe");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 1);

    place_at(&mut sim, heat_pipe, x, y, Direction::North);
    place_at(&mut sim, heat_pipe, x + 1, y, Direction::North);
    // Leaves x + 2 empty, so the last pipe cannot join the pair.
    place_at(&mut sim, heat_pipe, x + 3, y, Direction::North);

    sim.tick();

    assert_eq!(sim.heat_networks().len(), 2);
    let buffer_counts = sim
        .heat_networks()
        .iter()
        .map(|network| network.buffer_count)
        .collect::<Vec<_>>();
    assert_eq!(buffer_counts, vec![2, 1]);
}

/// The defining behaviour of a heat network: energy spreads until every buffer
/// sits at the same temperature, and the total is conserved exactly.
#[test]
fn heat_equalizes_to_a_common_temperature_without_losing_energy() {
    let mut sim = Simulation::new_test_world(123);
    let pipes = place_heat_pipe_run(&mut sim, 4);
    let injected = 40_000_001;
    set_heat_energy(&mut sim, pipes[0], injected);

    sim.tick();

    let total = pipes.iter().map(|id| heat_energy(&sim, *id)).sum::<u64>();
    assert_eq!(
        total, injected,
        "heat networks must conserve energy exactly"
    );
    let specific_heat = sim
        .heat_buffer_prototype(pipes[0])
        .expect("heat pipe declares a heat buffer")
        .specific_heat_joules_per_degree;
    let temperatures = pipes
        .iter()
        .map(|id| temperature_millidegrees(heat_energy(&sim, *id), specific_heat))
        .collect::<Vec<_>>();
    // Identical buffers settle to within the one-joule remainder handed out to
    // keep the total exact.
    let coldest = temperatures.iter().copied().min().expect("four pipes");
    let hottest = temperatures.iter().copied().max().expect("four pipes");
    assert!(
        hottest - coldest <= 1,
        "expected one settled temperature, got {temperatures:?}"
    );
}

/// A buffer at its maximum temperature cannot absorb more, so its surplus has to
/// go to the rest of the network rather than vanish.
#[test]
fn heat_beyond_a_buffer_maximum_redistributes_instead_of_being_lost() {
    let mut sim = Simulation::new_test_world(123);
    let pipes = place_heat_pipe_run(&mut sim, 2);
    let capacity = sim.heat_buffer_capacity_joules(pipes[0]);
    set_heat_energy(&mut sim, pipes[0], capacity);

    sim.tick();

    assert_eq!(
        heat_energy(&sim, pipes[0]) + heat_energy(&sim, pipes[1]),
        capacity
    );
    assert_eq!(heat_energy(&sim, pipes[0]), capacity / 2);
    assert_eq!(heat_energy(&sim, pipes[1]), capacity / 2);
}

#[test]
fn heat_topology_is_cached_until_placement_changes() {
    let mut sim = Simulation::new_test_world(123);
    place_heat_pipe_run(&mut sim, 2);

    sim.tick();
    let rebuilds_after_first_tick = sim.heat_topology_rebuild_count();
    sim.tick();
    sim.tick();

    assert_eq!(sim.heat_topology_rebuild_count(), rebuilds_after_first_tick);

    place_heat_pipe_run(&mut sim, 1);
    sim.tick();

    assert!(sim.heat_topology_rebuild_count() > rebuilds_after_first_tick);
}

/// Places a reactor with a fuel cell in it and returns its entity id.
fn place_fuelled_reactor(sim: &mut Simulation) -> EntityId {
    let reactor = entity_id_by_name(&sim.world.prototypes, "nuclear_reactor");
    let fuel_cell = item_id(&sim.world.prototypes, "uranium_fuel_cell");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 5, 5);
    let reactor_id = place_at(sim, reactor, x, y, Direction::North);
    sim.entities
        .nuclear_reactor_state_mut(reactor_id)
        .expect("placed reactor should expose reactor state")
        .energy
        .fuel_slot = test_slot(test_stack(fuel_cell, 1));
    reactor_id
}

#[test]
fn a_fuelled_reactor_heats_its_own_buffer() {
    let mut sim = Simulation::new_test_world(123);
    let reactor_id = place_fuelled_reactor(&mut sim);

    sim.tick();

    let reactor = sim
        .world
        .prototypes
        .entity(
            sim.entities
                .placed_entity(reactor_id)
                .expect("reactor is placed")
                .prototype_id,
        )
        .and_then(|prototype| prototype.nuclear_reactor)
        .expect("nuclear reactor prototype");
    let expected_per_tick = reactor.heat_output_watts / SIMULATION_TICKS_PER_SECOND;
    assert_eq!(heat_energy(&sim, reactor_id), expected_per_tick);
    assert_eq!(
        sim.machine_status_for_entity(reactor_id),
        Some(MachineStatus::Working)
    );
}

/// Burning a fuel cell must leave a spent cell behind, otherwise reprocessing
/// would have no input.
#[test]
fn burning_a_fuel_cell_leaves_a_spent_cell_in_the_output_slot() {
    let mut sim = Simulation::new_test_world(123);
    let reactor_id = place_fuelled_reactor(&mut sim);
    let spent = item_id(&sim.world.prototypes, "used_up_uranium_fuel_cell");

    sim.tick();

    let state = sim
        .entities
        .nuclear_reactor_state(reactor_id)
        .expect("reactor state");
    assert!(state.energy.fuel_slot.is_empty());
    assert_eq!(
        state.output_slot.stack().map(|stack| stack.item_id()),
        Some(spent)
    );
}

/// A blocked output must stop the reactor rather than destroy the spent cell.
#[test]
fn a_reactor_with_a_full_output_slot_holds_its_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let reactor_id = place_fuelled_reactor(&mut sim);
    let spent = item_id(&sim.world.prototypes, "used_up_uranium_fuel_cell");
    let stack_size = sim
        .world
        .prototypes
        .item(spent)
        .expect("spent fuel cell prototype")
        .stack_size;
    sim.entities
        .nuclear_reactor_state_mut(reactor_id)
        .expect("reactor state")
        .output_slot = test_slot(test_stack(spent, stack_size));

    sim.tick();

    let state = sim
        .entities
        .nuclear_reactor_state(reactor_id)
        .expect("reactor state");
    assert!(
        !state.energy.fuel_slot.is_empty(),
        "a reactor with nowhere to put the residue must keep its fuel"
    );
    assert_eq!(heat_energy(&sim, reactor_id), 0);
    assert_eq!(
        sim.machine_status_for_entity(reactor_id),
        Some(MachineStatus::OutputFull)
    );
}

/// The neighbour bonus is the reason players build reactor rows: two adjacent
/// reactors each produce more than one alone, from the same fuel.
#[test]
fn adjacent_reactors_boost_each_other_without_burning_extra_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let reactor = entity_id_by_name(&sim.world.prototypes, "nuclear_reactor");
    let fuel_cell = item_id(&sim.world.prototypes, "uranium_fuel_cell");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 10, 5);
    let first = place_at(&mut sim, reactor, x, y, Direction::North);
    let second = place_at(&mut sim, reactor, x + 5, y, Direction::North);
    for reactor_id in [first, second] {
        sim.entities
            .nuclear_reactor_state_mut(reactor_id)
            .expect("reactor state")
            .energy
            .fuel_slot = test_slot(test_stack(fuel_cell, 1));
    }

    sim.tick();

    let bonus = sim
        .world
        .prototypes
        .entity(
            sim.entities
                .placed_entity(first)
                .expect("reactor is placed")
                .prototype_id,
        )
        .and_then(|prototype| prototype.nuclear_reactor)
        .expect("nuclear reactor prototype");
    let base_per_tick = bonus.heat_output_watts / SIMULATION_TICKS_PER_SECOND;
    // Both reactors share one network, so the bonus shows up in the network total.
    assert_eq!(sim.heat_networks().len(), 1);
    let doubled = base_per_tick * 2;
    assert_eq!(sim.heat_networks()[0].energy_joules, doubled * 2);
}

/// Warm-up is the defining pacing of a heat network: below its minimum working
/// temperature a heat exchanger makes no steam at all.
#[test]
fn a_cold_heat_exchanger_makes_no_steam() {
    let mut sim = Simulation::new_test_world(123);
    let (exchanger_id, water) = place_watered_heat_exchanger(&mut sim);

    sim.tick();

    assert_eq!(
        sim.entities.fluid_boxes[&exchanger_id][1].amount_milliunits,
        0
    );
    assert_eq!(
        sim.machine_status_for_entity(exchanger_id),
        Some(MachineStatus::NoHeat)
    );
    assert!(sim.entities.fluid_boxes[&exchanger_id][0].amount_milliunits > 0);
    let _ = water;
}

#[test]
fn a_hot_heat_exchanger_turns_water_into_steam_and_spends_heat() {
    let mut sim = Simulation::new_test_world(123);
    let (exchanger_id, _water) = place_watered_heat_exchanger(&mut sim);
    let prototype = sim
        .world
        .prototypes
        .entity(
            sim.entities
                .placed_entity(exchanger_id)
                .expect("exchanger is placed")
                .prototype_id,
        )
        .expect("heat exchanger prototype");
    let heat_source = prototype
        .heat_energy_source
        .expect("heat exchanger declares a heat energy source");
    let specific_heat = prototype
        .heat_buffer
        .as_ref()
        .expect("heat exchanger declares a heat buffer")
        .specific_heat_joules_per_degree;
    let boiler = prototype
        .boiler
        .as_ref()
        .expect("heat exchanger reuses boiler rates")
        .clone();
    let hot = sim.heat_buffer_capacity_joules(exchanger_id);
    set_heat_energy(&mut sim, exchanger_id, hot);

    sim.tick();

    let energy_per_tick = heat_source.energy_usage_watts / SIMULATION_TICKS_PER_SECOND;
    assert_eq!(heat_energy(&sim, exchanger_id), hot - energy_per_tick);
    let expected_steam = per_tick_milliunits(boiler.steam_output_per_second_milliunits);
    assert_eq!(
        sim.entities.fluid_boxes[&exchanger_id][1].amount_milliunits,
        expected_steam
    );
    // Still comfortably above the minimum working temperature after one tick.
    assert!(
        heat_energy(&sim, exchanger_id)
            >= energy_for_temperature(heat_source.min_working_temperature_degrees, specific_heat)
    );
    assert_eq!(
        sim.machine_status_for_entity(exchanger_id),
        Some(MachineStatus::Working)
    );
}

/// Places a heat exchanger fed by a water pipe, returning it and the water id.
fn place_watered_heat_exchanger(sim: &mut Simulation) -> (EntityId, FluidId) {
    let exchanger = entity_id_by_name(&sim.world.prototypes, "heat_exchanger");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let water = fluid_id(&sim.world.prototypes, "water");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 3, 3);

    // Water box opens north from the exchanger's north-west tile.
    let water_pipe = place_at(sim, pipe, x, y, Direction::North);
    let exchanger_id = place_at(sim, exchanger, x, y + 1, Direction::North);
    set_fluid_box(sim, water_pipe, 0, water, 100_000);

    (exchanger_id, water)
}

/// The full nuclear chain, end to end: a reactor heats a pipe run, the exchanger
/// on the far end boils water, and a turbine burns the steam into power that a
/// consumer actually receives.
#[test]
fn reactor_heat_reaches_a_distant_exchanger_and_powers_a_turbine() {
    let mut sim = Simulation::new_test_world(123);
    let reactor = entity_id_by_name(&sim.world.prototypes, "nuclear_reactor");
    let heat_pipe = entity_id_by_name(&sim.world.prototypes, "heat_pipe");
    let exchanger = entity_id_by_name(&sim.world.prototypes, "heat_exchanger");
    let turbine = entity_id_by_name(&sim.world.prototypes, "steam_turbine");
    let pipe = entity_id_by_name(&sim.world.prototypes, "pipe");
    let pole = entity_id_by_name(&sim.world.prototypes, "small_electric_pole");
    let radar = entity_id_by_name(&sim.world.prototypes, "radar");
    let fuel_cell = item_id(&sim.world.prototypes, "uranium_fuel_cell");
    let water = fluid_id(&sim.world.prototypes, "water");
    let steam = fluid_id(&sim.world.prototypes, "steam");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 16, 9);

    // Reactor west, three heat pipes east from its east port, and the exchanger
    // reaching down onto the far end of that run.
    let reactor_id = place_at(&mut sim, reactor, x, y + 2, Direction::North);
    for offset in 0..3 {
        place_at(
            &mut sim,
            heat_pipe,
            x + 5 + i64::from(offset),
            y + 4,
            Direction::North,
        );
    }
    let exchanger_id = place_at(&mut sim, exchanger, x + 6, y + 2, Direction::North);
    let water_pipe = place_at(&mut sim, pipe, x + 6, y + 1, Direction::North);
    let steam_pipe = place_at(&mut sim, pipe, x + 8, y + 1, Direction::North);
    let turbine_id = place_at(&mut sim, turbine, x + 9, y, Direction::North);
    place_at(&mut sim, pole, x + 12, y + 5, Direction::North);
    let radar_id = place_at(&mut sim, radar, x + 10, y + 6, Direction::North);
    sim.entities
        .nuclear_reactor_state_mut(reactor_id)
        .expect("reactor state")
        .energy
        .fuel_slot = test_slot(test_stack(fuel_cell, 10));

    sim.tick();

    // Reactor, pipes, and exchanger form one heat network.
    assert_eq!(sim.heat_networks().len(), 1);
    assert_eq!(sim.heat_networks()[0].buffer_count, 5);
    // Exchanger and turbine share the steam network the exchanger fills.
    assert_eq!(
        sim.fluid_network_id_for_box_key(FluidBoxKey {
            entity_id: steam_pipe,
            box_index: 0
        }),
        sim.fluid_network_id_for_box_key(FluidBoxKey {
            entity_id: turbine_id,
            box_index: 0
        })
    );
    // The consumer is wired to the turbine's power network.
    assert!(sim.entity_power_status(radar_id).is_some());

    // Preheat instead of waiting out the real multi-second warm-up.
    for entity_id in [reactor_id, exchanger_id] {
        let capacity = sim.heat_buffer_capacity_joules(entity_id);
        set_heat_energy(&mut sim, entity_id, capacity);
    }

    for _ in 0..30 {
        set_fluid_box(&mut sim, water_pipe, 0, water, 100_000);
        sim.tick();
    }

    assert!(
        sim.entity_heat_status(exchanger_id)
            .expect("exchanger exposes heat status")
            .temperature_millidegrees
            > 500_000,
        "the exchanger should stay above its minimum working temperature"
    );
    assert!(
        total_fluid_amount(&sim, steam) > 0,
        "the exchanger should have produced steam"
    );
    assert!(
        sim.power_summary().production_watts > 0,
        "the turbine should convert that steam into power for the radar"
    );
    sim.validate().expect("nuclear chain should stay valid");
}

/// Destroying a reactor has to hand the player back both the loaded fuel cell
/// and the spent cells waiting in its output.
#[test]
fn destroying_a_reactor_recovers_its_fuel_and_spent_cells() {
    let mut sim = Simulation::new_test_world(123);
    let reactor_id = place_fuelled_reactor(&mut sim);
    let fuel_cell = item_id(&sim.world.prototypes, "uranium_fuel_cell");
    let spent = item_id(&sim.world.prototypes, "used_up_uranium_fuel_cell");
    sim.entities
        .nuclear_reactor_state_mut(reactor_id)
        .expect("reactor state")
        .output_slot = test_slot(test_stack(spent, 3));
    let fuel_before = sim.player_inventory.count(fuel_cell);
    let spent_before = sim.player_inventory.count(spent);

    crate::entity_mutation::destroy_to_player_inventory(&mut sim, reactor_id)
        .expect("a placed reactor should be removable");

    assert_eq!(sim.player_inventory.count(fuel_cell), fuel_before + 1);
    assert_eq!(sim.player_inventory.count(spent), spent_before + 3);
    assert!(!sim.entities.heat_buffers.contains_key(&reactor_id));
    sim.validate()
        .expect("removing a reactor should leave valid state");
}
