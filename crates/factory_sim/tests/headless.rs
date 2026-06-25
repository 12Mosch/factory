use factory_sim::Simulation;

#[test]
fn sim_runs_3600_ticks_without_bevy() {
    let mut sim = Simulation::new_test_world(123);
    for _ in 0..3600 {
        sim.tick();
    }
}

#[test]
fn same_seed_same_hash_after_ticks() {
    let mut a = Simulation::new_test_world(42);
    let mut b = Simulation::new_test_world(42);

    for _ in 0..10_000 {
        a.tick();
        b.tick();
    }

    assert_eq!(a.state_hash(), b.state_hash());
}
