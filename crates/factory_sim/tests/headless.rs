use factory_sim::Simulation;

#[test]
fn sim_runs_3600_ticks_without_bevy() {
    let mut sim = Simulation::new_test_world(123);
    for _ in 0..3600 {
        sim.tick();
    }
}
