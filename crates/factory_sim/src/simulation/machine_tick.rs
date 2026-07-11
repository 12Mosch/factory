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
        MachineTickContext {
            world: &mut self.world,
            entities: &mut self.entities,
            transport: &mut self.transport,
            research: &mut self.research,
            power: &mut self.power,
            statistics: StatisticsContext::new(self.tick, &mut self.statistics),
            early_game_progress: &mut self.early_game_progress,
        }
    }

    pub(super) fn advance_machines<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut context = self.machine_tick_context();
        context.advance_burner_mining_drills(profiler);
        // No `profiler`: pumpjacks only touch their own fluid boxes and call
        // none of the sub-phase-profiled helpers (inventory transfers, resource
        // scans, ...). Their cost is already counted under ProfilePhase::Machines.
        context.advance_pumpjacks();
        context.advance_furnaces(profiler);
        context.advance_assembling_machines(profiler);
        context.advance_labs(profiler);
    }

    pub(super) fn advance_inserters<P: TickProfiler>(&mut self, profiler: &mut P) {
        self.machine_tick_context().advance_inserters(profiler);
    }
}
