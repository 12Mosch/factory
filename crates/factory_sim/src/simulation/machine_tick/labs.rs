use super::progress::{ProgressAdvance, advance_electric_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_labs<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut labs = std::mem::take(&mut self.entities.labs);

        for (&entity_id, state) in &mut labs {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some(technology_id) = self.research.active else {
                state.active_technology = None;
                state.progress_ticks = 0;
                state.required_ticks = 0;
                continue;
            };

            let Some(technology) = self.world.prototypes.technology(technology_id) else {
                state.active_technology = None;
                state.progress_ticks = 0;
                state.required_ticks = 0;
                continue;
            };
            let required_ticks = technology.research_time_ticks;
            let science_packs = technology.science_packs.as_slice();

            if state.active_technology != Some(technology_id) {
                state.active_technology = Some(technology_id);
                state.progress_ticks = 0;
            }
            state.required_ticks = required_ticks;
            let can_work = profiler.measure(ProfilePhase::InventoryTransfers, || {
                lab_has_science_packs(&state.inventory, science_packs)
            });
            if !can_work {
                continue;
            }
            if !electric_work_allowed_for(
                self.power,
                &mut self.entities.electric_consumers,
                entity_id,
            ) {
                continue;
            }

            let completed = matches!(
                advance_electric_progress(&mut state.progress_ticks, required_ticks),
                ProgressAdvance::Completed
            );
            if !completed {
                continue;
            }

            profiler.measure(ProfilePhase::InventoryTransfers, || {
                for science_pack in science_packs {
                    state
                        .inventory
                        .remove(science_pack.item, science_pack.amount)
                        .expect("lab checked science packs before completion");
                }
            });
            for science_pack in science_packs {
                self.statistics
                    .record_item_consumed(science_pack.item, u64::from(science_pack.amount));
            }
            self.add_research_units(1)
                .expect("lab completion should have active research");
        }

        self.entities.labs = labs;
    }
}
