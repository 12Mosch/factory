use super::progress::{ProgressAdvance, advance_electric_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_assembling_machines<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut assembling_machines = std::mem::take(&mut self.entities.assembling_machines);

        for (&entity_id, state) in &mut assembling_machines {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some(recipe) =
                selected_assembler_recipe(&self.world.prototypes, self.research, state)
            else {
                state.crafting_progress_ticks = 0;
                state.crafting_required_ticks = 0;
                continue;
            };
            let ingredients = recipe.ingredients.as_slice();
            let products = recipe.products.as_slice();
            let required_ticks = assembler_required_ticks(
                recipe.crafting_time_ticks,
                state.crafting_speed_numerator,
                state.crafting_speed_denominator,
            );

            let can_craft = profiler.measure(ProfilePhase::InventoryTransfers, || {
                assembler_has_ingredients(&state.input_inventory, ingredients)
                    && assembler_output_can_accept(
                        &self.world.prototypes,
                        &state.output_inventory,
                        products,
                    )
            });
            if !can_craft {
                state.crafting_required_ticks = required_ticks;
                continue;
            }
            state.crafting_required_ticks = required_ticks;
            if !electric_work_allowed_for(
                self.power,
                &mut self.entities.electric_consumers,
                entity_id,
            ) {
                continue;
            }

            let completed = matches!(
                advance_electric_progress(&mut state.crafting_progress_ticks, required_ticks),
                ProgressAdvance::Completed
            );

            if !completed {
                continue;
            }

            profiler.measure(ProfilePhase::InventoryTransfers, || {
                for ingredient in ingredients {
                    state
                        .input_inventory
                        .remove(ingredient.item, ingredient.amount)
                        .expect("assembler checked ingredients before completion");
                }
                for product in products {
                    state
                        .output_inventory
                        .insert(&self.world.prototypes, product.item, product.amount)
                        .expect("assembler checked output capacity before completion");
                }
            });
            for ingredient in ingredients {
                self.statistics
                    .record_item_consumed(ingredient.item, u64::from(ingredient.amount));
            }
            for product in products {
                self.statistics
                    .record_item_produced(product.item, u64::from(product.amount));
            }
        }

        self.entities.assembling_machines = assembling_machines;
    }
}
