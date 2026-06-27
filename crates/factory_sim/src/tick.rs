use crate::simulation::{NoopTickProfiler, ProfilePhase, Simulation, TickProfiler};

pub fn advance_simulation(sim: &mut Simulation) {
    let mut profiler = NoopTickProfiler;
    advance_simulation_profiled(sim, &mut profiler);
}

pub(crate) fn advance_simulation_profiled<P: TickProfiler>(sim: &mut Simulation, profiler: &mut P) {
    sim.advance_one_tick(profiler);
    #[cfg(debug_assertions)]
    {
        profiler.measure(ProfilePhase::Validation, || sim.validate().unwrap());
    }
}
