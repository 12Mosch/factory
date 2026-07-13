use super::progress::{ProgressAdvance, advance_electric_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_assembling_machines<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut assembling_machines = std::mem::take(&mut self.entities.assembling_machines);

        for (&entity_id, state) in &mut assembling_machines {
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };

            let Some(recipe) =
                selected_assembler_recipe(&self.world.prototypes, self.research, state)
            else {
                state.crafting_progress_ticks = 0;
                state.crafting_required_ticks = 0;
                continue;
            };
            let ingredients = recipe.ingredients.as_slice();
            let products = recipe.products.as_slice();
            let fluid_ingredients = recipe.fluid_ingredients.as_slice();
            let fluid_products = recipe.fluid_products.as_slice();
            let required_ticks = assembler_required_ticks(
                recipe.crafting_time_ticks,
                state.crafting_speed_numerator,
                state.crafting_speed_denominator,
            );

            let can_craft_items = profiler.measure(ProfilePhase::InventoryTransfers, || {
                assembler_has_ingredients(&state.input_inventory, ingredients)
                    && assembler_output_can_accept(
                        &self.world.prototypes,
                        &state.output_inventory,
                        products,
                    )
            });
            let fluid_assignment = if fluid_ingredients.is_empty() && fluid_products.is_empty() {
                Some((Vec::new(), Vec::new()))
            } else {
                let box_states = self
                    .entities
                    .fluid_boxes
                    .get(&entity_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                fluid_ingredient_box_indices(&prototype.fluid_boxes, box_states, fluid_ingredients)
                    .zip(fluid_product_box_indices(
                        &prototype.fluid_boxes,
                        box_states,
                        fluid_products,
                    ))
            };

            state.crafting_required_ticks = required_ticks;
            let Some((ingredient_boxes, product_boxes)) =
                can_craft_items.then_some(fluid_assignment).flatten()
            else {
                continue;
            };
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
            self.pollution_emitters.mark_active(entity_id);

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
            if !fluid_ingredients.is_empty() || !fluid_products.is_empty() {
                let box_states = self
                    .entities
                    .fluid_boxes
                    .get_mut(&entity_id)
                    .expect("fluid recipe availability was checked before completion");
                consume_fluid_ingredients(box_states, &ingredient_boxes, fluid_ingredients);
                insert_fluid_products(box_states, &product_boxes, fluid_products);
            }
            // Recipe slices borrow prototypes here, so record through the field
            // instead of taking a mutable borrow of the whole tick context.
            for ingredient in ingredients {
                self.statistics
                    .record_item_consumed(ingredient.item, u64::from(ingredient.amount));
            }
            for product in products {
                self.statistics
                    .record_item_produced(product.item, u64::from(product.amount));
                self.onboarding_progress.record_item_produced(
                    &self.base,
                    product.item,
                    u64::from(product.amount),
                );
                self.onboarding_progress.record_counter(
                    |progress| &mut progress.assembler_items_produced,
                    u64::from(product.amount),
                );
            }
            for ingredient in fluid_ingredients {
                self.statistics
                    .record_fluid_consumed(ingredient.fluid, ingredient.amount_milliunits);
            }
            for product in fluid_products {
                self.statistics
                    .record_fluid_produced(product.fluid, product.amount_milliunits);
                if product.fluid == self.base.fluids.petroleum_gas {
                    self.onboarding_progress.record_counter(
                        |progress| &mut progress.petroleum_gas_produced,
                        product.amount_milliunits / 1_000,
                    );
                }
            }
        }

        self.entities.assembling_machines = assembling_machines;
    }
}
