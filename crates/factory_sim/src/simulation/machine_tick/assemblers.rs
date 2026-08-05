use super::crafting::{CraftProducts, ItemCraft, record_item_craft};
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
            let fluid_ingredients = recipe.fluid_ingredients.as_slice();
            let fluid_products = recipe.fluid_products.as_slice();
            let required_ticks = state.crafting_required_ticks;
            let output_copies = state.modules.output_copies_due();

            let can_craft_items = profiler.measure(ProfilePhase::InventoryTransfers, || {
                ItemCraft {
                    input_inventory: &mut state.input_inventory,
                    products: CraftProducts::Inventory(&mut state.output_inventory),
                }
                .can_craft(&self.world.prototypes, recipe, output_copies)
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
                    .zip(fluid_product_box_indices_for_copies(
                        &prototype.fluid_boxes,
                        box_states,
                        fluid_products,
                        output_copies,
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
            let bonus_copies = state.modules.complete_productive_cycle();
            debug_assert_eq!(output_copies, 1 + bonus_copies);

            profiler.measure(ProfilePhase::InventoryTransfers, || {
                ItemCraft {
                    input_inventory: &mut state.input_inventory,
                    products: CraftProducts::Inventory(&mut state.output_inventory),
                }
                .complete(&self.world.prototypes, recipe, output_copies)
            });
            if !fluid_ingredients.is_empty() || !fluid_products.is_empty() {
                let box_states = self
                    .entities
                    .fluid_boxes
                    .get_mut(&entity_id)
                    .expect("fluid recipe availability was checked before completion");
                consume_fluid_ingredients(box_states, &ingredient_boxes, fluid_ingredients);
                insert_fluid_product_copies(
                    box_states,
                    &product_boxes,
                    fluid_products,
                    output_copies,
                );
                for &box_index in ingredient_boxes.iter().chain(&product_boxes) {
                    self.fluids
                        .mark_box_dirty(FluidBoxKey::entity(entity_id, box_index));
                }
            }
            // Recipe slices borrow prototypes here, so record through the fields
            // instead of taking a mutable borrow of the whole tick context.
            record_item_craft(
                &mut self.statistics,
                self.onboarding_progress,
                &self.base,
                recipe,
                output_copies,
            );
            for product in &recipe.products {
                self.onboarding_progress.record_counter(
                    |progress| &mut progress.assembler_items_produced,
                    u64::from(product.amount).saturating_mul(output_copies),
                );
            }
            for ingredient in fluid_ingredients {
                self.statistics
                    .record_fluid_consumed(ingredient.fluid, ingredient.amount_milliunits);
            }
            for product in fluid_products {
                self.statistics.record_fluid_produced(
                    product.fluid,
                    product.amount_milliunits.saturating_mul(output_copies),
                );
                if product.fluid == self.base.fluids.petroleum_gas {
                    self.onboarding_progress.record_counter(
                        |progress| &mut progress.petroleum_gas_produced,
                        product.amount_milliunits.saturating_mul(output_copies) / 1_000,
                    );
                }
            }
            self.power_demand_cache.mark_dirty(entity_id);
        }

        self.entities.assembling_machines = assembling_machines;
    }
}
