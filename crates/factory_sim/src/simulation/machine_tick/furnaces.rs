use super::progress::{ProgressAdvance, advance_burner_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_furnaces<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut furnaces = std::mem::take(&mut self.entities.furnaces);

        for (&entity_id, state) in &mut furnaces {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some((recipe_id, required_ticks, ingredient, product)) =
                furnace_work_selection(&self.world.prototypes, self.research, state.input_slot)
            else {
                state.active_recipe = None;
                state.crafting_progress_ticks = 0;
                state.crafting_required_ticks = 0;
                continue;
            };

            let output_can_accept = profiler.measure(ProfilePhase::InventoryTransfers, || {
                output_slot_can_accept(
                    &self.world.prototypes,
                    state.output_slot,
                    product.item,
                    product.amount,
                )
            });
            if !output_can_accept {
                if state.active_recipe != Some(recipe_id) {
                    state.crafting_progress_ticks = 0;
                }
                state.active_recipe = Some(recipe_id);
                state.crafting_required_ticks = required_ticks;
                continue;
            }

            if state.active_recipe != Some(recipe_id) {
                state.active_recipe = Some(recipe_id);
                state.crafting_progress_ticks = 0;
                state.crafting_required_ticks = required_ticks;
            }

            let advance = advance_burner_progress(
                &self.world.prototypes,
                &mut state.energy,
                &mut state.crafting_progress_ticks,
                required_ticks,
                profiler,
            );
            if let Some(item_id) = advance.consumed_fuel {
                self.record_item_consumed(item_id, 1);
            }

            if !matches!(advance.result, ProgressAdvance::Completed) {
                continue;
            }

            profiler.measure(ProfilePhase::InventoryTransfers, || {
                remove_from_single_slot(&mut state.input_slot, ingredient.item, ingredient.amount)
                    .expect("selected furnace input should still contain ingredient");
                insert_output_item(&mut state.output_slot, product.item, product.amount);
            });
            self.record_item_consumed(ingredient.item, u64::from(ingredient.amount));
            self.record_item_produced(product.item, u64::from(product.amount));
        }

        self.entities.furnaces = furnaces;
    }
}
