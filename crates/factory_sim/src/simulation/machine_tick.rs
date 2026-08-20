use super::*;

mod assemblers;
mod crafting;
mod furnaces;
mod inserters;
mod labs;
mod mining_drills;
mod progress;
mod pumpjacks;
mod rocket_silos;

impl Simulation {
    fn machine_tick_context(&mut self) -> MachineTickContext<'_> {
        let base = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        let rocket_silo_recipe = ResolvedRocketSiloRecipe::for_entities(
            &self.world.prototypes,
            &self.research,
            &self.entities,
        );
        let mining_drill_productivity_permyriad = self
            .research
            .bonuses(&self.world.prototypes)
            .mining_drill_productivity_permyriad;
        MachineTickContext {
            world: &mut self.world,
            entities: &mut self.entities,
            rolling_stock: &mut self.rolling_stock,
            stopped_stock: &self.stopped_stock_index,
            transport: &mut self.transport,
            research: &mut self.research,
            rocket_silo_recipe,
            mining_drill_productivity_permyriad,
            power: &mut self.power,
            power_demand_cache: &mut self.power_demand_cache,
            fluids: &mut self.fluids,
            circuits: &self.circuits,
            statistics: StatisticsContext::new(self.tick, &mut self.statistics),
            onboarding_progress: &mut self.onboarding_progress,
            pollution_emitters: &mut self.pollution_emitters,
            base,
        }
    }

    pub(super) fn advance_machines<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut context = self.machine_tick_context();
        context.advance_mining_drills(profiler);
        // No `profiler`: pumpjacks only touch their own fluid boxes and call
        // none of the sub-phase-profiled helpers (inventory transfers, resource
        // scans, ...). Their cost is already counted under ProfilePhase::Machines.
        context.advance_pumpjacks();
        context.advance_furnaces(profiler);
        context.advance_assembling_machines(profiler);
        context.advance_rocket_silos(profiler);
        context.advance_labs(profiler);
    }

    pub(super) fn advance_inserters<P: TickProfiler>(&mut self, profiler: &mut P) {
        self.machine_tick_context().advance_inserters(profiler);
    }
}
