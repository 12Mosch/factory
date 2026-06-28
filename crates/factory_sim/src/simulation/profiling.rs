use super::*;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimulationCounts {
    pub entity_count: usize,
    pub chunk_count: usize,
    pub belt_count: usize,
    pub belt_item_count: usize,
    pub machine_count: usize,
    pub inserter_count: usize,
    pub active_machines: usize,
    pub idle_machines: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimulationTickProfile {
    pub total: Duration,
    pub entity_motion: Duration,
    pub belts: Duration,
    pub machines: Duration,
    pub inserters: Duration,
    pub inventory_transfers: Duration,
    pub chunk_lookup: Duration,
    pub manual_crafting: Duration,
    pub validation: Duration,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ProfilePhase {
    EntityMotion,
    Belts,
    Machines,
    Inserters,
    InventoryTransfers,
    ChunkLookup,
    ManualCrafting,
    Validation,
    Total,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ProfileSpan {
    Noop,
    Started(Instant),
}

pub(crate) trait TickProfiler {
    fn begin(&mut self) -> ProfileSpan;
    fn finish(&mut self, phase: ProfilePhase, span: ProfileSpan);

    fn measure<R>(&mut self, phase: ProfilePhase, f: impl FnOnce() -> R) -> R {
        let span = self.begin();
        let result = f();
        self.finish(phase, span);
        result
    }
}

#[derive(Default)]
pub(crate) struct NoopTickProfiler;

impl TickProfiler for NoopTickProfiler {
    fn begin(&mut self) -> ProfileSpan {
        ProfileSpan::Noop
    }

    fn finish(&mut self, _phase: ProfilePhase, _span: ProfileSpan) {}

    fn measure<R>(&mut self, _phase: ProfilePhase, f: impl FnOnce() -> R) -> R {
        f()
    }
}

#[derive(Default)]
pub(super) struct TickProfileCollector {
    profile: SimulationTickProfile,
}

impl TickProfileCollector {
    pub(super) fn into_profile(self) -> SimulationTickProfile {
        self.profile
    }
}

impl TickProfiler for TickProfileCollector {
    fn begin(&mut self) -> ProfileSpan {
        ProfileSpan::Started(Instant::now())
    }

    fn finish(&mut self, phase: ProfilePhase, span: ProfileSpan) {
        let ProfileSpan::Started(started) = span else {
            return;
        };
        let elapsed = started.elapsed();
        match phase {
            ProfilePhase::EntityMotion => self.profile.entity_motion += elapsed,
            ProfilePhase::Belts => self.profile.belts += elapsed,
            ProfilePhase::Machines => self.profile.machines += elapsed,
            ProfilePhase::Inserters => self.profile.inserters += elapsed,
            ProfilePhase::InventoryTransfers => self.profile.inventory_transfers += elapsed,
            ProfilePhase::ChunkLookup => self.profile.chunk_lookup += elapsed,
            ProfilePhase::ManualCrafting => self.profile.manual_crafting += elapsed,
            ProfilePhase::Validation => self.profile.validation += elapsed,
            ProfilePhase::Total => self.profile.total += elapsed,
        }
    }
}

impl Simulation {
    pub fn counts(&self) -> SimulationCounts {
        let machine_count = self.entities.burner_mining_drills.len()
            + self.entities.furnaces.len()
            + self.entities.assembling_machines.len()
            + self.entities.labs.len();
        let active_machines = self.active_machine_count();

        SimulationCounts {
            entity_count: self.entities.len() + self.entities.placed_len(),
            chunk_count: self.world.chunks.len(),
            belt_count: self.entities.transport_belts.len() + self.entities.splitters.len(),
            belt_item_count: self
                .entities
                .transport_belts
                .values()
                .map(|segment| {
                    segment
                        .lanes
                        .iter()
                        .map(|lane| lane.items.len())
                        .sum::<usize>()
                })
                .sum::<usize>()
                + self
                    .entities
                    .splitters
                    .values()
                    .map(|state| {
                        state
                            .input_lanes
                            .iter()
                            .flat_map(|input_lanes| input_lanes.iter())
                            .map(|lane| lane.items.len())
                            .sum::<usize>()
                    })
                    .sum::<usize>(),
            machine_count,
            inserter_count: self.entities.inserters.len(),
            active_machines,
            idle_machines: machine_count.saturating_sub(active_machines),
        }
    }

    pub fn profiled_tick(&mut self) -> SimulationTickProfile {
        let mut profiler = TickProfileCollector::default();
        let span = profiler.begin();
        crate::tick::advance_simulation_profiled(self, &mut profiler);
        profiler.finish(ProfilePhase::Total, span);
        profiler.into_profile()
    }

    fn active_machine_count(&self) -> usize {
        self.entities
            .burner_mining_drills
            .iter()
            .filter(|(entity_id, state)| self.burner_mining_drill_is_active(**entity_id, state))
            .count()
            + self
                .entities
                .furnaces
                .values()
                .filter(|state| self.furnace_is_active(state))
                .count()
            + self
                .entities
                .assembling_machines
                .values()
                .filter(|state| self.assembler_is_active(state))
                .count()
            + self
                .entities
                .labs
                .values()
                .filter(|state| self.lab_is_active(state))
                .count()
    }

    fn burner_mining_drill_is_active(
        &self,
        entity_id: EntityId,
        state: &BurnerMiningDrillState,
    ) -> bool {
        if state.energy.fuel_slot.is_some()
            || state.energy.energy_remaining_joules > f64::EPSILON
            || state.mining_progress_ticks > 0
        {
            return true;
        }

        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return false;
        };
        let Some(prototype) = self
            .world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
        else {
            return false;
        };
        let Some(mining_drill) = prototype.mining_drill.as_ref() else {
            return false;
        };
        let Some((_, resource_item)) =
            first_resource_in_mining_area(&self.world, &placed.footprint, mining_drill)
        else {
            return false;
        };
        let output_target = drill_output_target(&self.entities, placed);

        drill_output_target_can_accept(
            &self.world.prototypes,
            &self.entities,
            output_target,
            state.output_slot,
            resource_item,
            1,
        )
    }

    fn furnace_is_active(&self, state: &FurnaceState) -> bool {
        let Some(recipe_id) = state.active_recipe else {
            return false;
        };
        let Some(recipe) = self
            .world
            .prototypes
            .recipes
            .get(recipe_id.index())
            .filter(|recipe| recipe.id == recipe_id && recipe.products.len() == 1)
        else {
            return false;
        };
        let product = &recipe.products[0];

        output_slot_can_accept(
            &self.world.prototypes,
            state.output_slot,
            product.item,
            product.amount,
        )
    }

    fn assembler_is_active(&self, state: &AssemblingMachineState) -> bool {
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, state) else {
            return false;
        };

        assembler_has_ingredients(&state.input_inventory, &recipe.ingredients)
            && assembler_output_can_accept(
                &self.world.prototypes,
                &state.output_inventory,
                &recipe.products,
            )
    }

    fn lab_is_active(&self, state: &LabState) -> bool {
        let Some(technology_id) = state.active_technology.or(self.research.active) else {
            return false;
        };
        let Some(technology) = self
            .world
            .prototypes
            .technologies
            .get(technology_id.index())
            .filter(|technology| technology.id == technology_id)
        else {
            return false;
        };

        lab_has_science_packs(&state.inventory, &technology.science_packs)
    }
}
