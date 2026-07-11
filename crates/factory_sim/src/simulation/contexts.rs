use super::research_ops::add_research_units_to_state;
use super::statistics_ops::{ItemStatisticDirection, StatisticDirection, add_stat, subtract_stat};
use super::*;

pub(super) struct StatisticsContext<'a> {
    tick: u64,
    statistics: &'a mut StatisticsSubsystem,
}

impl<'a> StatisticsContext<'a> {
    pub(super) fn new(tick: u64, statistics: &'a mut StatisticsSubsystem) -> Self {
        Self { tick, statistics }
    }

    pub(super) fn advance_to_current_tick(&mut self) {
        while self.statistics.items.last_advanced_tick < self.tick {
            self.statistics.items.last_advanced_tick += 1;
            self.clear_item_statistics_bucket(self.statistics.items.last_advanced_tick);
        }
        while self.statistics.fluids.last_advanced_tick < self.tick {
            self.statistics.fluids.last_advanced_tick += 1;
            self.clear_fluid_statistics_bucket(self.statistics.fluids.last_advanced_tick);
        }
        while self.statistics.power.last_advanced_tick < self.tick {
            self.statistics.power.last_advanced_tick += 1;
            self.clear_power_statistics_sample(self.statistics.power.last_advanced_tick);
        }
    }

    pub(super) fn record_item_produced(&mut self, item_id: ItemId, amount: u64) {
        self.record_item_stat(item_id, amount, ItemStatisticDirection::Produced);
    }

    pub(super) fn record_item_consumed(&mut self, item_id: ItemId, amount: u64) {
        self.record_item_stat(item_id, amount, ItemStatisticDirection::Consumed);
    }

    fn record_item_stat(
        &mut self,
        item_id: ItemId,
        amount: u64,
        direction: ItemStatisticDirection,
    ) {
        if amount == 0 {
            return;
        }
        self.advance_to_current_tick();
        self.ensure_current_item_statistics_bucket();

        let index = self.current_statistics_bucket_index();
        let bucket = &mut self.statistics.items.buckets[index];
        match direction {
            ItemStatisticDirection::Produced => {
                add_stat(&mut bucket.produced, item_id, amount);
                add_stat(&mut self.statistics.items.rolling_produced, item_id, amount);
                add_stat(&mut self.statistics.items.total_produced, item_id, amount);
            }
            ItemStatisticDirection::Consumed => {
                add_stat(&mut bucket.consumed, item_id, amount);
                add_stat(&mut self.statistics.items.rolling_consumed, item_id, amount);
                add_stat(&mut self.statistics.items.total_consumed, item_id, amount);
            }
        }
    }

    pub(super) fn record_fluid_produced(&mut self, fluid_id: FluidId, amount: u64) {
        self.record_fluid_stat(fluid_id, amount, StatisticDirection::Produced);
    }

    pub(super) fn record_fluid_consumed(&mut self, fluid_id: FluidId, amount: u64) {
        self.record_fluid_stat(fluid_id, amount, StatisticDirection::Consumed);
    }

    fn record_fluid_stat(&mut self, fluid_id: FluidId, amount: u64, direction: StatisticDirection) {
        if amount == 0 {
            return;
        }
        self.advance_to_current_tick();
        self.ensure_current_fluid_statistics_bucket();

        let index = self.current_statistics_bucket_index();
        let bucket = &mut self.statistics.fluids.buckets[index];
        match direction {
            StatisticDirection::Produced => {
                add_stat(&mut bucket.produced, fluid_id, amount);
                add_stat(
                    &mut self.statistics.fluids.rolling_produced,
                    fluid_id,
                    amount,
                );
                add_stat(&mut self.statistics.fluids.total_produced, fluid_id, amount);
            }
            StatisticDirection::Consumed => {
                add_stat(&mut bucket.consumed, fluid_id, amount);
                add_stat(
                    &mut self.statistics.fluids.rolling_consumed,
                    fluid_id,
                    amount,
                );
                add_stat(&mut self.statistics.fluids.total_consumed, fluid_id, amount);
            }
        }
    }

    pub(super) fn record_power_sample(&mut self, summary: PowerSummary) {
        self.advance_to_current_tick();
        let index = self.current_statistics_bucket_index();
        self.statistics.power.samples[index] = PowerStatisticsSample {
            tick: self.tick,
            production_watts: summary.production_watts,
            available_production_watts: summary.available_production_watts,
            consumption_watts: summary.consumption_watts,
            satisfaction_permyriad: summary.satisfaction_permyriad,
        };
    }

    fn ensure_current_item_statistics_bucket(&mut self) {
        let index = self.current_statistics_bucket_index();
        if self.statistics.items.buckets[index].tick != self.tick {
            self.clear_item_statistics_bucket(self.tick);
        }
    }

    fn ensure_current_fluid_statistics_bucket(&mut self) {
        let index = self.current_statistics_bucket_index();
        if self.statistics.fluids.buckets[index].tick != self.tick {
            self.clear_fluid_statistics_bucket(self.tick);
        }
    }

    fn clear_item_statistics_bucket(&mut self, tick: u64) {
        let index = (tick % ITEM_STATISTICS_WINDOW_TICKS) as usize;
        let bucket = &mut self.statistics.items.buckets[index];
        for (item_id, amount) in std::mem::take(&mut bucket.produced) {
            subtract_stat(&mut self.statistics.items.rolling_produced, item_id, amount);
        }
        for (item_id, amount) in std::mem::take(&mut bucket.consumed) {
            subtract_stat(&mut self.statistics.items.rolling_consumed, item_id, amount);
        }
        bucket.tick = tick;
    }

    fn clear_fluid_statistics_bucket(&mut self, tick: u64) {
        let index = (tick % ITEM_STATISTICS_WINDOW_TICKS) as usize;
        let bucket = &mut self.statistics.fluids.buckets[index];
        for (fluid_id, amount) in std::mem::take(&mut bucket.produced) {
            subtract_stat(
                &mut self.statistics.fluids.rolling_produced,
                fluid_id,
                amount,
            );
        }
        for (fluid_id, amount) in std::mem::take(&mut bucket.consumed) {
            subtract_stat(
                &mut self.statistics.fluids.rolling_consumed,
                fluid_id,
                amount,
            );
        }
        bucket.tick = tick;
    }

    fn clear_power_statistics_sample(&mut self, tick: u64) {
        let index = (tick % ITEM_STATISTICS_WINDOW_TICKS) as usize;
        self.statistics.power.samples[index] = PowerStatisticsSample {
            tick,
            ..PowerStatisticsSample::default()
        };
    }

    fn current_statistics_bucket_index(&self) -> usize {
        (self.tick % ITEM_STATISTICS_WINDOW_TICKS) as usize
    }
}

pub(super) struct MachineTickContext<'a> {
    pub(super) world: &'a mut WorldSim,
    pub(super) entities: &'a mut EntityStore,
    pub(super) transport: &'a mut TransportLaneCache,
    pub(super) research: &'a mut ResearchState,
    pub(super) power: &'a mut PowerSubsystem,
    pub(super) statistics: StatisticsContext<'a>,
    pub(super) onboarding_progress: &'a mut OnboardingProgress,
    pub(super) base: factory_data::BasePrototypeIds,
}

impl<'a> MachineTickContext<'a> {
    pub(super) fn electric_work_allowed(&mut self, entity_id: EntityId) -> bool {
        electric_work_allowed_for(self.power, &mut self.entities.electric_consumers, entity_id)
    }

    pub(super) fn record_item_produced(&mut self, item_id: ItemId, amount: u64) {
        self.statistics.record_item_produced(item_id, amount);
        self.onboarding_progress
            .record_item_produced(&self.base, item_id, amount);
    }

    pub(super) fn record_item_consumed(&mut self, item_id: ItemId, amount: u64) {
        self.statistics.record_item_consumed(item_id, amount);
    }

    pub(super) fn add_research_units(
        &mut self,
        units: u32,
    ) -> Result<ResearchProgressResult, ResearchError> {
        let result = add_research_units_to_state(&self.world.prototypes, self.research, units)?;
        if let ResearchProgressResult::Completed { technology_id } = result
            && let Some(technology) = self.world.prototypes.technology(technology_id)
        {
            self.onboarding_progress
                .record_research_completed(&technology.name);
        }
        Ok(result)
    }
}

pub(super) fn electric_work_allowed_for(
    power: &PowerSubsystem,
    electric_consumers: &mut BTreeMap<EntityId, ElectricConsumerState>,
    entity_id: EntityId,
) -> bool {
    let satisfaction_permyriad = power
        .entity_statuses
        .get(&entity_id)
        .map(|status| status.satisfaction_permyriad)
        .unwrap_or(0);
    if satisfaction_permyriad == 0 {
        return false;
    }

    let Some(state) = electric_consumers.get_mut(&entity_id) else {
        return true;
    };
    if satisfaction_permyriad >= POWER_SATISFACTION_FULL_PERMYRIAD {
        state.work_remainder_permyriad = 0;
        return true;
    }

    state.work_remainder_permyriad = state
        .work_remainder_permyriad
        .saturating_add(satisfaction_permyriad);
    if state.work_remainder_permyriad >= POWER_SATISFACTION_FULL_PERMYRIAD {
        state.work_remainder_permyriad -= POWER_SATISFACTION_FULL_PERMYRIAD;
        true
    } else {
        false
    }
}

pub(super) struct PowerContext<'a> {
    pub(super) power: &'a mut PowerSubsystem,
}

impl<'a> PowerContext<'a> {
    pub(super) fn new(power: &'a mut PowerSubsystem) -> Self {
        Self { power }
    }

    pub(super) fn invalidate_power_state(&mut self) {
        self.power.topology_dirty = true;
        self.invalidate_power_dynamic_state();
    }

    pub(super) fn invalidate_power_dynamic_state(&mut self) {
        self.power.summary = PowerSummary {
            satisfaction_permyriad: POWER_SATISFACTION_FULL_PERMYRIAD,
            ..PowerSummary::default()
        };
        self.power.networks.clear();
        self.power.entity_statuses.clear();
    }
}
