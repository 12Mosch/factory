use super::*;

mod assemblers;
mod burner_drills;
mod furnaces;
mod inserters;
mod labs;
mod progress;
mod pumpjacks;

impl Simulation {
    fn machine_tick_context(&mut self) -> MachineTickContext<'_> {
        MachineTickContext::new(
            self.tick,
            &mut self.world,
            &mut self.entities,
            &mut self.transport,
            &mut self.research,
            &mut self.power,
            &mut self.statistics,
        )
    }

    pub(super) fn advance_machines<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut context = self.machine_tick_context();
        context.advance_burner_mining_drills(profiler);
        context.advance_pumpjacks();
        context.advance_furnaces(profiler);
        context.advance_assembling_machines(profiler);
        context.advance_labs(profiler);
    }

    pub(super) fn advance_inserters<P: TickProfiler>(&mut self, profiler: &mut P) {
        self.machine_tick_context().advance_inserters(profiler);
    }
}
