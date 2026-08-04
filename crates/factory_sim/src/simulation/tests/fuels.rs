//! The refined fuels: where they come from, and that they burn everywhere a
//! burner burns.
//!
//! Solid fuel and rocket fuel are ordinary items with a `fuel_value_joules`,
//! so nothing in the burner path knows their names. That is exactly why they
//! are worth testing from the outside: what has to hold is that every machine
//! which accepts coal accepts them too, and that the only difference the
//! player sees is how long one item lasts.

use super::super::*;
use super::rolling_stock::{fuel_train_with, world_with_a_driveable_locomotive};
use super::support::*;
use crate::rolling_stock::{TrainId, TrainThrottle};

/// Joules a locomotive spends per tick, from its 600 kW burner.
const LOCOMOTIVE_JOULES_PER_TICK: f64 = 600_000.0 / FIXED_SIM_TICKS_PER_SECOND_F64;

/// Ceiling on the loop that measures how long one item of fuel lasts. The
/// longest leg is one rocket fuel at 10 000 driving ticks, so this is only
/// reached when a locomotive has stopped spending fuel altogether.
const MAX_MEASURED_DRIVING_TICKS: u32 = 15_000;

fn burner_energy_remaining(sim: &Simulation, entity_id: EntityId) -> f64 {
    crate::entity_access::furnace_state(sim, entity_id)
        .expect("furnace should expose state")
        .energy
        .burner()
        .expect("a stone furnace is a burner machine")
        .energy_remaining_joules
}

#[test]
fn chemical_plant_turns_light_oil_into_solid_fuel() {
    let mut sim = Simulation::new_test_world(123);
    unlock_with_prerequisites(&mut sim, "solid_fuel");
    let plant_id = place_powered_chemical_plant(&mut sim);
    let light_oil = fluid_id(&sim.world.prototypes, "light_oil");
    let solid_fuel = item_id(&sim.world.prototypes, "solid_fuel");
    let recipe = recipe_id(&sim.world.prototypes, "solid_fuel");

    sim.select_assembler_recipe(plant_id, recipe)
        .expect("chemical plant should accept the solid fuel recipe");
    set_fluid_box(&mut sim, plant_id, 0, light_oil, 30_000);

    for _ in 0..400 {
        sim.tick();
    }

    // Three crafts of ten light oil each, and the light oil is gone rather
    // than cracked back into the gas it came from.
    assert_eq!(fluid_box_amount(&sim, plant_id, 0), (None, 0));
    let state = sim
        .entities
        .assembler_state(plant_id)
        .expect("chemical plant should expose assembler state");
    assert_eq!(state.output_inventory.count(solid_fuel), 3);
}

#[test]
fn chemical_plant_packs_solid_fuel_and_light_oil_into_rocket_fuel() {
    let mut sim = Simulation::new_test_world(123);
    unlock_with_prerequisites(&mut sim, "rocket_fuel");
    let plant_id = place_powered_chemical_plant(&mut sim);
    let light_oil = fluid_id(&sim.world.prototypes, "light_oil");
    let solid_fuel = item_id(&sim.world.prototypes, "solid_fuel");
    let rocket_fuel = item_id(&sim.world.prototypes, "rocket_fuel");
    let recipe = recipe_id(&sim.world.prototypes, "rocket_fuel");

    sim.select_assembler_recipe(plant_id, recipe)
        .expect("chemical plant should accept the rocket fuel recipe");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, solid_fuel, 10);
    crate::entity_transfer::player_slot_to_assembler_input(&mut sim, plant_id, 0)
        .expect("chemical plant should accept solid fuel as an ingredient");
    set_fluid_box(&mut sim, plant_id, 0, light_oil, 10_000);

    for _ in 0..1_800 {
        sim.tick();
    }

    assert_eq!(fluid_box_amount(&sim, plant_id, 0), (None, 0));
    let state = sim
        .entities
        .assembler_state(plant_id)
        .expect("chemical plant should expose assembler state");
    assert_eq!(state.input_inventory.count(solid_fuel), 0);
    assert_eq!(state.output_inventory.count(rocket_fuel), 1);
}

/// A furnace banks the fuel value of whatever is in its fuel slot. Coal is the
/// same test one rung down, so the two numbers below are the whole difference
/// between the rungs: same furnace, same 210-tick smelt, three times the fuel
/// left over.
#[test]
fn furnace_smelts_on_solid_fuel_and_banks_its_larger_fuel_value() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let solid_fuel = item_id(&sim.world.prototypes, "solid_fuel");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, solid_fuel);

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_plate, 1)));
    // 12 MJ banked, less the 90 kW the furnace drew for 210 ticks.
    assert_eq!(burner_energy_remaining(&sim, entity_id), 11_685_000.0);
}

#[test]
fn furnace_smelts_on_rocket_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    let rocket_fuel = item_id(&sim.world.prototypes, "rocket_fuel");
    let entity_id = place_stone_furnace(&mut sim);
    add_furnace_input_and_fuel(&mut sim, entity_id, iron_ore, rocket_fuel);

    for _ in 0..210 {
        sim.tick();
    }

    let state =
        crate::entity_access::furnace_state(&sim, entity_id).expect("furnace should expose state");
    assert_eq!(state.output_slot.stack(), Some(test_stack(iron_plate, 1)));
    assert_eq!(burner_energy_remaining(&sim, entity_id), 99_685_000.0);
}

#[test]
fn boiler_raises_steam_on_solid_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let (_, _, boiler_id) = place_powered_fixture_origin_with_boiler(&mut sim, 1, 1, (1, 2));
    let solid_fuel = item_id(&sim.world.prototypes, "solid_fuel");
    let steam = fluid_id(&sim.world.prototypes, "steam");
    sim.entities
        .boiler_state_mut(boiler_id)
        .expect("placed boiler should expose boiler state")
        .energy
        .fuel_slot = test_slot(test_stack(solid_fuel, 1));

    sim.tick();

    let boiler = crate::entity_access::boiler_state(&sim, boiler_id).expect("boiler should exist");
    assert_eq!(
        boiler.energy.fuel_slot.stack(),
        None,
        "the one item is burnt"
    );
    // 12 MJ banked, less the 1.8 MW the boiler drew for one tick.
    assert_eq!(boiler.energy.energy_remaining_joules, 11_970_000.0);
    let (fluid, amount) = fluid_box_amount(&sim, boiler_id, 1);
    assert_eq!(fluid, Some(steam));
    assert!(amount > 0, "a fuelled, watered boiler makes steam");
}

#[test]
fn burner_inserter_swings_on_rocket_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let rocket_fuel = item_id(&sim.world.prototypes, "rocket_fuel");
    let (x, y) = first_buildable_rect_without_resource(&sim.world, 4, 2);
    let (chest_id, inserter_id, furnace_id) =
        place_chest_inserter_furnace_line_at(&mut sim, "burner_inserter", x, y);
    set_inventory_slot(
        crate::entity_access::inventory_mut(&mut sim, chest_id)
            .expect("chest should have inventory"),
        0,
        iron_ore,
        1,
    );
    sim.entities
        .inserter_energy_mut(inserter_id)
        .expect("burner inserter should expose energy")
        .fuel_slot_mut()
        .expect("burner inserter should expose a fuel slot")
        .insert_stack(&sim.world.prototypes, test_stack(rocket_fuel, 1))
        .expect("rocket fuel should fit the burner inserter fuel slot");

    run_inserter_until_idle(&mut sim, inserter_id);

    assert_eq!(
        crate::entity_access::furnace_state(&sim, furnace_id)
            .expect("furnace should have state")
            .input_slot,
        Some(test_stack(iron_ore, 1))
    );
    let energy = crate::entity_access::inserter_energy(&sim, inserter_id)
        .expect("burner inserter should expose energy")
        .burner()
        .expect("burner inserter should use burner energy");
    assert!(
        energy.energy_remaining_joules > 0.0,
        "one rocket fuel is far more than one swing costs"
    );
}

#[test]
fn burner_mining_drill_runs_on_solid_fuel() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let solid_fuel = item_id(&sim.world.prototypes, "solid_fuel");
    let (drill_id, chest_id, _, _, _) = place_burner_drill_outputting_to_chest(&mut sim, iron_ore);
    add_fuel_to_burner_drill(&mut sim, drill_id, solid_fuel, 1);

    for _ in 0..240 {
        sim.tick();
    }

    assert_eq!(
        crate::entity_access::inventory(&sim, chest_id)
            .expect("chest should have inventory")
            .count(iron_ore),
        1,
        "a solid-fuelled drill should mine into its chest"
    );
}

/// What a better fuel actually buys a locomotive, pinned rather than assumed.
///
/// Tractive force is a property of the locomotive, not of what is in its fuel
/// slot: a train on rocket fuel pulls exactly as hard as one on coal. What
/// changes is the range — a locomotive spends 10 kJ per driving tick whatever
/// it is burning, so an item lasts as long as its fuel value divided by that,
/// which is the whole reason to haul rocket fuel to a refuelling stop.
#[test]
fn a_locomotive_pulls_the_same_on_every_fuel_but_runs_longer_on_a_better_one() {
    let (mut sim, _, _, train_id) = world_with_a_driveable_locomotive();
    let fuels =
        ["coal", "solid_fuel", "rocket_fuel"].map(|name| item_id(&sim.world.prototypes, name));

    let tractive_force = |sim: &Simulation, train_id: TrainId| {
        sim.train_forces_now(train_id)
            .expect("the train exists")
            .tractive_force_newtons
    };

    let mut forces = Vec::new();
    let mut driving_ticks = Vec::new();
    for fuel in fuels {
        fuel_train_with(&mut sim, train_id, fuel, 1);
        sim.set_train_throttle(train_id, TrainThrottle::Coast)
            .expect("the train takes a throttle command");
        sim.tick();
        forces.push(tractive_force(&sim, train_id));

        sim.set_train_throttle(train_id, TrainThrottle::Forward)
            .expect("the train takes a throttle command");
        let mut ticks = 0;
        // The locomotive runs out of track long before it runs out of fuel;
        // the throttle stays open either way, which is what keeps burning.
        // Bounded rather than a bare `while`: a regression that stopped a
        // throttled locomotive spending fuel would otherwise hang the suite
        // instead of failing it.
        while tractive_force(&sim, train_id) > 0 {
            assert!(
                ticks < MAX_MEASURED_DRIVING_TICKS,
                "a throttled locomotive should burn one item dry within \
                 {MAX_MEASURED_DRIVING_TICKS} ticks"
            );
            sim.tick();
            ticks += 1;
        }
        driving_ticks.push(ticks);
    }

    assert_eq!(
        forces,
        vec![12_000, 12_000, 12_000],
        "fuel buys range, not force: every fuel pulls with the locomotive's own tractive force"
    );
    let expected = fuels.map(|fuel| {
        let joules = sim
            .world
            .prototypes
            .item(fuel)
            .and_then(|item| item.fuel_value_joules)
            .expect("every fuel in the ladder has a fuel value");
        (joules as f64 / LOCOMOTIVE_JOULES_PER_TICK) as u32
    });
    assert_eq!(driving_ticks, expected.to_vec());
    assert_eq!(driving_ticks, vec![400, 1_200, 10_000]);
}
