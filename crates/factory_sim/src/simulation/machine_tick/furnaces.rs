use super::progress::{MachineEnergyContext, ProgressAdvance, advance_machine_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_furnaces<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut furnaces = std::mem::take(&mut self.entities.furnaces);

        for (&entity_id, state) in &mut furnaces {
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };

            let Some((recipe_id, recipe_ticks, ingredient, product)) =
                furnace_work_selection(&self.world.prototypes, self.research, state.input_slot)
            else {
                state.crafting_progress_ticks = 0;
                continue;
            };
            let recipe_changed = state.active_recipe != Some(recipe_id);
            if recipe_changed {
                state.modules.productivity_progress_permyriad = 0;
            }
            let required_ticks = if recipe_changed {
                let furnace = prototype
                    .furnace
                    .as_ref()
                    .expect("validated furnace has crafting metadata");
                required_ticks_with_modules(
                    recipe_ticks,
                    furnace.crafting_speed_numerator,
                    furnace.crafting_speed_denominator,
                    state.modules.resolved_effects,
                )
            } else {
                state.crafting_required_ticks
            };
            let output_copies = state.modules.output_copies_due();
            let output_amount = u64::from(product.amount).saturating_mul(output_copies);

            let output_can_accept = profiler.measure(ProfilePhase::InventoryTransfers, || {
                u16::try_from(output_amount).is_ok_and(|amount| {
                    state
                        .output_slot
                        .can_insert_item(&self.world.prototypes, product.item, amount)
                })
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

            let advance = advance_machine_progress(
                MachineEnergyContext {
                    catalog: &self.world.prototypes,
                    power: self.power,
                    electric_consumers: &mut self.entities.electric_consumers,
                    entity_id,
                    energy_multiplier_permyriad: state
                        .modules
                        .resolved_effects
                        .energy_multiplier_permyriad(),
                },
                &mut state.energy,
                &mut state.crafting_progress_ticks,
                required_ticks,
                profiler,
            );
            if let Some(item_id) = advance.consumed_fuel {
                self.record_item_consumed(item_id, 1);
            }
            if !matches!(advance.result, ProgressAdvance::Blocked) {
                self.mark_pollution_emitter_active(entity_id);
            }

            if !matches!(advance.result, ProgressAdvance::Completed) {
                continue;
            }
            state.modules.complete_productive_cycle();

            profiler.measure(ProfilePhase::InventoryTransfers, || {
                state
                    .input_slot
                    .remove(ingredient.item, ingredient.amount)
                    .expect("selected furnace input should still contain ingredient");
                state
                    .output_slot
                    .insert(&self.world.prototypes, product.item, output_amount as u16)
                    .expect("the checked furnace output slot should accept the product");
            });
            self.record_item_consumed(ingredient.item, u64::from(ingredient.amount));
            self.record_item_produced(product.item, output_amount);
            self.power_demand_cache.mark_dirty(entity_id);
            if product.item == self.base.items.iron_plate {
                self.onboarding_progress
                    .record_counter(|progress| &mut progress.iron_plates_smelted, output_amount);
            }
        }

        self.entities.furnaces = furnaces;
    }
}
