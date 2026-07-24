use super::super::*;
use super::support::*;

/// Places two wire-linked small electric poles forming one network and returns
/// the origin of the surrounding clear area. Callers place solar panels,
/// accumulators, and consumers at documented offsets that are covered by the
/// poles' supply areas without colliding with them.
fn build_network(sim: &mut Simulation) -> (WorldTileCoord, WorldTileCoord) {
    let (ox, oy) = first_buildable_rect_without_resource(&sim.world, 14, 8);
    place_named(sim, "small_electric_pole", ox + 3, oy + 4);
    place_named(sim, "small_electric_pole", ox + 9, oy + 4);
    (ox, oy)
}

fn place_named(sim: &mut Simulation, name: &str, x: WorldTileCoord, y: WorldTileCoord) -> EntityId {
    let prototype_id = entity_id_by_name(&sim.world.prototypes, name);
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id,
            x,
            y,
            direction: Direction::North,
        },
    )
    .unwrap_or_else(|error| panic!("{name} should be placeable: {error:?}"))
}

/// Solar panel covered by the left pole (see [`build_network`]).
fn place_solar(sim: &mut Simulation, ox: WorldTileCoord, oy: WorldTileCoord) -> EntityId {
    place_named(sim, "solar_panel", ox, oy + 2)
}

/// Accumulator covered by the left pole.
fn place_accumulator(sim: &mut Simulation, ox: WorldTileCoord, oy: WorldTileCoord) -> EntityId {
    place_named(sim, "accumulator", ox + 4, oy + 2)
}

/// Assembler covered by the right pole, wired to the same network.
fn place_consumer(sim: &mut Simulation, ox: WorldTileCoord, oy: WorldTileCoord) -> EntityId {
    place_named(sim, "assembling_machine", ox + 10, oy + 2)
}

/// Selects a recipe and loads ingredients so the assembler actively demands its
/// full electric usage.
fn make_consumer_active(sim: &mut Simulation, assembler_id: EntityId) {
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("assembler should accept recipe");
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 100);
    crate::entity_transfer::player_slot_to_assembler_input(sim, assembler_id, 0)
        .expect("assembler should accept ingredients");
}

fn set_stored_energy(sim: &mut Simulation, accumulator_id: EntityId, joules: u64) {
    sim.entities
        .accumulators
        .get_mut(&accumulator_id)
        .expect("accumulator state should exist")
        .stored_energy_joules = joules;
}

#[test]
fn full_daylight_solar_reaches_max_output() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);

    sim.tick();

    assert_eq!(sim.daylight_ratio(), (1, 1));
    let summary = sim.power_summary();
    // No consumers, so all 60 kW of solar shows up as available capability.
    assert_eq!(summary.available_production_watts, 60_000);
    assert_eq!(summary.production_watts, 0);
    assert_eq!(summary.consumption_watts, 0);
}

/// A generated test world on a short day/night cycle (day 0..10, dusk 10..14,
/// night 14..16, dawn 16..20) so tests can reach dusk and night in a handful of
/// ticks while keeping the tick-count/phase invariant intact.
fn sim_with_short_cycle(seed: u64) -> Simulation {
    let mut catalog = PrototypeCatalog::load_base().expect("base catalog should load");
    catalog.day_night_cycle = Some(factory_data::DayNightCycleConfig {
        cycle_length_ticks: 20,
        dawn_dusk_ticks: 4,
    });
    Simulation::new(seed, catalog)
}

#[test]
fn solar_output_scales_with_partial_daylight() {
    let mut sim = sim_with_short_cycle(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);

    // Tick to phase 12: mid-dusk, a 2/4 daylight ratio.
    for _ in 0..12 {
        sim.tick();
    }

    let (numerator, denominator) = sim.daylight_ratio();
    assert_eq!((numerator, denominator), (2, 4));
    let expected = 60_000 * numerator / denominator;
    assert_eq!(expected, 30_000);
    assert_eq!(sim.power_summary().available_production_watts, expected);
}

#[test]
fn night_solar_produces_no_power() {
    let mut sim = sim_with_short_cycle(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);

    // Tick to phase 15: full night.
    for _ in 0..15 {
        sim.tick();
    }

    assert_eq!(sim.daylight_ratio(), (0, 1));
    assert_eq!(sim.power_summary().available_production_watts, 0);
}

#[test]
fn disconnected_solar_produces_no_network_power() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = first_buildable_rect_without_resource(&sim.world, 6, 6);
    // A solar panel with no pole never joins a network.
    place_named(&mut sim, "solar_panel", ox, oy);

    sim.tick();

    assert_eq!(sim.power_summary().available_production_watts, 0);
    assert!(sim.power_networks().is_empty());
}

#[test]
fn solar_surplus_charges_accumulator() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    let accumulator_id = place_accumulator(&mut sim, ox, oy);

    sim.tick();

    // No consumers: the full 60 kW of solar charges the accumulator, adding
    // 60000 watt-ticks = 1000 J in one tick.
    let network = sim.power_networks()[0];
    assert_eq!(network.accumulator_charge_watts, 60_000);
    assert_eq!(network.accumulator_discharge_watts, 0);
    let state = crate::entity_access::accumulator_state(&sim, accumulator_id)
        .expect("accumulator should expose state");
    assert_eq!(state.stored_energy_joules(), 1_000);
    assert_eq!(state.energy_remainder_watt_ticks(), 0);
}

#[test]
fn ordinary_demand_is_served_before_charging() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    place_accumulator(&mut sim, ox, oy);
    let assembler_id = place_consumer(&mut sim, ox, oy);
    // Leave the assembler idle so its demand is only the 2.5 kW drain, well
    // under the 60 kW of solar.

    sim.tick();

    let network = sim.power_networks()[0];
    assert_eq!(network.consumption_watts, 2_500);
    assert_eq!(network.production_watts, 2_500);
    // Solar covers the drain first; only the 57.5 kW surplus charges storage.
    assert_eq!(network.accumulator_charge_watts, 57_500);
    assert_eq!(network.accumulator_discharge_watts, 0);
    assert_eq!(
        sim.entity_power_status(assembler_id)
            .expect("assembler should report status")
            .satisfaction_permyriad,
        POWER_SATISFACTION_FULL_PERMYRIAD
    );
}

#[test]
fn accumulator_discharges_only_after_generation_is_exhausted() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    let accumulator_id = place_accumulator(&mut sim, ox, oy);
    let assembler_id = place_consumer(&mut sim, ox, oy);
    make_consumer_active(&mut sim, assembler_id);
    set_stored_energy(&mut sim, accumulator_id, 1_000_000);

    sim.tick();

    let network = sim.power_networks()[0];
    // Active demand is 75 kW usage + 2.5 kW drain = 77.5 kW. Solar covers
    // 60 kW; the accumulator discharges the remaining 17.5 kW.
    assert_eq!(network.consumption_watts, 77_500);
    assert_eq!(network.accumulator_charge_watts, 0);
    assert_eq!(network.accumulator_discharge_watts, 17_500);
    assert_eq!(network.production_watts, 77_500);
    assert_eq!(
        network.satisfaction_permyriad,
        POWER_SATISFACTION_FULL_PERMYRIAD
    );
}

#[test]
fn full_accumulator_does_not_charge() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    let accumulator_id = place_accumulator(&mut sim, ox, oy);
    let capacity = sim
        .world
        .prototypes
        .entity(
            sim.entities
                .placed_entity(accumulator_id)
                .expect("accumulator placed")
                .prototype_id,
        )
        .and_then(|prototype| prototype.accumulator.as_ref())
        .expect("accumulator prototype")
        .capacity_joules;
    set_stored_energy(&mut sim, accumulator_id, capacity);

    sim.tick();

    let network = sim.power_networks()[0];
    assert_eq!(network.accumulator_charge_watts, 0);
    assert_eq!(network.accumulator_discharge_watts, 0);
    let state = crate::entity_access::accumulator_state(&sim, accumulator_id)
        .expect("accumulator should expose state");
    assert_eq!(state.stored_energy_joules(), capacity);
}

#[test]
fn steam_only_network_charges_accumulator_without_solar() {
    let mut sim = Simulation::new_test_world(123);
    // The boiler fixture wires an offshore pump, boiler, and steam engine into
    // one network; add an accumulator covered by the fixture's target pole.
    let (fx, fy, _boiler) = place_powered_fixture_origin_with_boiler(&mut sim, 3, 3, (3, 1));
    let accumulator_id = place_named(&mut sim, "accumulator", fx, fy);

    for _ in 0..3 {
        sim.tick();
    }

    let state = crate::entity_access::accumulator_state(&sim, accumulator_id)
        .expect("accumulator should expose state");
    assert!(
        state.stored_energy_joules() > 0,
        "steam surplus should charge the accumulator"
    );
}

#[test]
fn disconnected_networks_do_not_share_storage() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    place_accumulator(&mut sim, ox, oy);
    // A second, far-away network with only an idle accumulator: no generation,
    // so its storage never changes.
    let (fx, fy) = first_buildable_rect_without_resource(&sim.world, 6, 6);
    let far_pole_x = fx + 40;
    let solo_pole = place_named(&mut sim, "small_electric_pole", far_pole_x, fy);
    let solo_accumulator = place_named(&mut sim, "accumulator", far_pole_x + 1, fy);
    set_stored_energy(&mut sim, solo_accumulator, 500_000);

    sim.tick();

    assert_ne!(
        sim.entity_power_status(solo_pole)
            .and_then(|status| status.network_id),
        Some(0),
        "the far network should be distinct from network 0"
    );
    let state = crate::entity_access::accumulator_state(&sim, solo_accumulator)
        .expect("accumulator should expose state");
    assert_eq!(state.stored_energy_joules(), 500_000);
}

#[test]
fn solar_output_triggers_electricity_onboarding() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);

    assert!(!sim.onboarding_progress().electricity_generated);
    sim.tick();
    assert!(sim.onboarding_progress().electricity_generated);
}

#[test]
fn stored_discharge_alone_does_not_trigger_onboarding() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    let accumulator_id = place_accumulator(&mut sim, ox, oy);
    let assembler_id = place_consumer(&mut sim, ox, oy);
    make_consumer_active(&mut sim, assembler_id);
    set_stored_energy(&mut sim, accumulator_id, 1_000_000);

    sim.tick();

    // The accumulator discharges to power the assembler, but discharge is not
    // newly generated electricity.
    assert!(sim.power_networks()[0].accumulator_discharge_watts > 0);
    assert!(!sim.onboarding_progress().electricity_generated);
}

#[test]
fn changing_stored_charge_does_not_recompute_consumer_demand() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    place_accumulator(&mut sim, ox, oy);
    let assembler_id = place_consumer(&mut sim, ox, oy);
    make_consumer_active(&mut sim, assembler_id);

    // Prime the demand cache.
    sim.tick();
    let baseline = sim.power_demand_cache.demand_recomputations;
    // Storage charges every tick, but ordinary consumer demand is unchanged.
    for _ in 0..10 {
        sim.tick();
    }
    assert_eq!(sim.power_demand_cache.demand_recomputations, baseline);
}

#[test]
fn save_load_preserves_accumulator_energy_and_hash_mid_charge() {
    let mut sim = Simulation::new_test_world(123);
    let (ox, oy) = build_network(&mut sim);
    place_solar(&mut sim, ox, oy);
    let accumulator_id = place_accumulator(&mut sim, ox, oy);

    for _ in 0..25 {
        sim.tick();
    }
    let stored_before = crate::entity_access::accumulator_state(&sim, accumulator_id)
        .expect("accumulator state")
        .stored_energy_joules();
    assert!(stored_before > 0);

    let bytes = crate::save_to_bytes(&sim).expect("simulation should save");
    let mut loaded = crate::load_from_bytes(&bytes).expect("simulation should load");
    assert_eq!(loaded.state_hash(), sim.state_hash());
    assert_eq!(
        crate::entity_access::accumulator_state(&loaded, accumulator_id)
            .expect("loaded accumulator state")
            .stored_energy_joules(),
        stored_before
    );

    for _ in 0..25 {
        sim.tick();
        loaded.tick();
        assert_eq!(loaded.state_hash(), sim.state_hash());
    }
}
