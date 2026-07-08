use super::progress::{ProgressAdvance, advance_electric_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_labs<P: TickProfiler>(&mut self, profiler: &mut P) {
        let lab_ids = self.entities.labs.keys().copied().collect::<Vec<_>>();

        for entity_id in lab_ids {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some(technology_id) = self.research.active else {
                if let Ok(state) = self.entities.lab_state_mut(entity_id) {
                    state.active_technology = None;
                    state.progress_ticks = 0;
                    state.required_ticks = 0;
                }
                continue;
            };

            let Some(technology) = self.world.prototypes.technology(technology_id) else {
                if let Ok(state) = self.entities.lab_state_mut(entity_id) {
                    state.active_technology = None;
                    state.progress_ticks = 0;
                    state.required_ticks = 0;
                }
                continue;
            };
            let required_ticks = technology.research_time_ticks;
            let science_packs = technology.science_packs.clone();

            let can_work = {
                let state = self
                    .entities
                    .lab_state_mut(entity_id)
                    .expect("lab id came from lab state map");
                if state.active_technology != Some(technology_id) {
                    state.active_technology = Some(technology_id);
                    state.progress_ticks = 0;
                }
                state.required_ticks = required_ticks;
                profiler.measure(ProfilePhase::InventoryTransfers, || {
                    lab_has_science_packs(&state.inventory, &science_packs)
                })
            };
            if !can_work {
                continue;
            }
            if !self.electric_work_allowed(entity_id) {
                continue;
            }

            let completed = {
                let state = self
                    .entities
                    .lab_state_mut(entity_id)
                    .expect("lab id came from lab state map");
                matches!(
                    advance_electric_progress(&mut state.progress_ticks, required_ticks),
                    ProgressAdvance::Completed
                )
            };
            if !completed {
                continue;
            }

            let state = self
                .entities
                .lab_state_mut(entity_id)
                .expect("lab id came from lab state map");
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                for science_pack in &science_packs {
                    state
                        .inventory
                        .remove(science_pack.item, science_pack.amount)
                        .expect("lab checked science packs before completion");
                }
            });
            for science_pack in &science_packs {
                self.record_item_consumed(science_pack.item, u64::from(science_pack.amount));
            }
            self.add_research_units(1)
                .expect("lab completion should have active research");
        }
    }
}
