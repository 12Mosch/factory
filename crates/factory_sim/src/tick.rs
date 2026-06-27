use crate::simulation::Simulation;

pub fn advance_simulation(sim: &mut Simulation) {
    sim.advance_one_tick();
    #[cfg(debug_assertions)]
    sim.validate().unwrap();
}
