use super::super::*;
use super::support::*;
use std::time::Duration;

#[test]
fn counts_report_entities_chunks_belts_items_machines_and_inserters() {
    let mut sim = Simulation::new_test_world(123);
    let iron_ore = item_id(&sim.world.prototypes, "iron_ore");
    let (belt_id, _inserter_id, _furnace_id) = place_belt_inserter_furnace_line(&mut sim);

    sim.insert_item_onto_belt(belt_id, 0, iron_ore)
        .expect("empty belt should accept one item");

    let counts = sim.counts();
    assert_eq!(counts.entity_count, 9);
    assert_eq!(counts.chunk_count, 25);
    assert_eq!(counts.belt_count, 1);
    assert_eq!(counts.belt_item_count, 1);
    assert_eq!(counts.machine_count, 1);
    assert_eq!(counts.inserter_count, 1);
    assert_eq!(counts.active_machines, 0);
    assert_eq!(counts.idle_machines, 1);
}

#[test]
fn profiled_tick_advances_one_tick_and_reports_total_time() {
    let mut sim = Simulation::new_test_world(123);
    let before_tick = sim.tick_count();

    let profile = sim.profiled_tick();

    assert_eq!(sim.tick_count(), before_tick + 1);
    assert!(profile.total > Duration::ZERO);
}

#[test]
fn profiled_tick_preserves_deterministic_hashes_against_tick() {
    let mut ticked = Simulation::new_test_world(123);
    let mut profiled = Simulation::new_test_world(123);

    for _ in 0..120 {
        ticked.tick();
        profiled.profiled_tick();
        assert_eq!(profiled.state_hash(), ticked.state_hash());
    }
}
