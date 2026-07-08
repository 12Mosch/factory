use super::progress::{ProgressAdvance, advance_electric_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_assembling_machines<P: TickProfiler>(&mut self, profiler: &mut P) {
        let assembler_ids = self
            .entities
            .assembling_machines
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in assembler_ids {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some((ingredients, products, required_ticks)) = self
                .entities
                .assembler_state(entity_id)
                .ok()
                .and_then(|state| {
                    let recipe =
                        selected_assembler_recipe(&self.world.prototypes, self.research, state)?;
                    Some((
                        recipe.ingredients.clone(),
                        recipe.products.clone(),
                        assembler_required_ticks(
                            recipe.crafting_time_ticks,
                            state.crafting_speed_numerator,
                            state.crafting_speed_denominator,
                        ),
                    ))
                })
            else {
                if let Ok(state) = self.entities.assembler_state_mut(entity_id) {
                    state.crafting_progress_ticks = 0;
                    state.crafting_required_ticks = 0;
                }
                continue;
            };

            let can_craft = self.entities.assembler_state(entity_id).is_ok_and(|state| {
                profiler.measure(ProfilePhase::InventoryTransfers, || {
                    assembler_has_ingredients(&state.input_inventory, &ingredients)
                        && assembler_output_can_accept(
                            &self.world.prototypes,
                            &state.output_inventory,
                            &products,
                        )
                })
            });
            if !can_craft {
                if let Ok(state) = self.entities.assembler_state_mut(entity_id) {
                    state.crafting_required_ticks = required_ticks;
                }
                continue;
            }
            if let Ok(state) = self.entities.assembler_state_mut(entity_id) {
                state.crafting_required_ticks = required_ticks;
            }
            if !self.electric_work_allowed(entity_id) {
                continue;
            }

            let completed = {
                let state = self
                    .entities
                    .assembler_state_mut(entity_id)
                    .expect("assembler id came from assembler state map");
                state.crafting_required_ticks = required_ticks;
                matches!(
                    advance_electric_progress(&mut state.crafting_progress_ticks, required_ticks),
                    ProgressAdvance::Completed
                )
            };

            if !completed {
                continue;
            }

            let state = self
                .entities
                .assembler_state_mut(entity_id)
                .expect("assembler id came from assembler state map");
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                for ingredient in &ingredients {
                    state
                        .input_inventory
                        .remove(ingredient.item, ingredient.amount)
                        .expect("assembler checked ingredients before completion");
                }
                for product in &products {
                    state
                        .output_inventory
                        .insert(&self.world.prototypes, product.item, product.amount)
                        .expect("assembler checked output capacity before completion");
                }
            });
            for ingredient in &ingredients {
                self.record_item_consumed(ingredient.item, u64::from(ingredient.amount));
            }
            for product in &products {
                self.record_item_produced(product.item, u64::from(product.amount));
            }
        }
    }
}
