use super::crafting::{CraftProducts, ItemCraft, record_item_craft};
use super::progress::{ProgressAdvance, advance_electric_progress};
use super::*;
use crate::machines::rocket_silo::{LAUNCH_RISE_TICKS, LAUNCH_SEAL_TICKS, RocketLaunchPhase};
use crate::simulation::module_ops::rescale_progress;

impl MachineTickContext<'_> {
    /// Advances every rocket silo by one tick.
    ///
    /// The shape is the assembler's, and the two differences are both visible
    /// here: the recipe is derived from the catalog rather than read off the
    /// state, and the products go to [`CraftProducts::RocketParts`] rather than
    /// to an output inventory. A silo holding a whole rocket fails
    /// [`ItemCraft::can_craft`] on the room check and so accumulates no progress
    /// and draws no power until the rocket leaves.
    pub(super) fn advance_rocket_silos<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut rocket_silos = std::mem::take(&mut self.entities.rocket_silos);

        for (&entity_id, state) in &mut rocket_silos {
            let Some(silo_prototype) = self
                .entities
                .placed_entity(entity_id)
                .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
                .and_then(|prototype| prototype.rocket_silo)
            else {
                continue;
            };
            let launch_product = silo_prototype.launch_product;

            match state.launch_phase {
                RocketLaunchPhase::Idle
                    if state.rocket_ready()
                        && state.has_launch_payload(silo_prototype.launch_payload)
                        && state.output_inventory.can_insert(
                            &self.world.prototypes,
                            launch_product.item,
                            launch_product.amount,
                        ) =>
                {
                    state.launch_phase = RocketLaunchPhase::Sealed {
                        ticks_remaining: LAUNCH_SEAL_TICKS,
                    };
                    continue;
                }
                RocketLaunchPhase::Sealed { ticks_remaining: 1 } => {
                    state.launch_phase = RocketLaunchPhase::Rising {
                        ticks_remaining: LAUNCH_RISE_TICKS,
                    };
                    continue;
                }
                RocketLaunchPhase::Sealed { ticks_remaining } => {
                    state.launch_phase = RocketLaunchPhase::Sealed {
                        ticks_remaining: ticks_remaining - 1,
                    };
                    continue;
                }
                RocketLaunchPhase::Rising { ticks_remaining: 1 } => {
                    if !state.has_launch_payload(silo_prototype.launch_payload) {
                        // Launch state is externally constructible through
                        // saves. Never mint a reward unless the payload that
                        // justified this launch is still present and exact.
                        continue;
                    }
                    if state
                        .output_inventory
                        .insert(
                            &self.world.prototypes,
                            launch_product.item,
                            launch_product.amount,
                        )
                        .is_err()
                    {
                        // Output can only become emptier after launch starts,
                        // but a malformed externally-constructed state should
                        // stall safely rather than destroy its launch reward.
                        continue;
                    }
                    let cargo = state
                        .cargo_inventory
                        .take_slot(0)
                        .expect("a launching rocket retains its validated cargo");
                    self.record_item_produced(
                        launch_product.item,
                        u64::from(launch_product.amount),
                    );
                    self.statistics
                        .record_item_consumed(cargo.item_id(), u64::from(cargo.count()));
                    self.statistics.record_rocket_launched();
                    state.parts_completed = 0;
                    state.launch_phase = RocketLaunchPhase::Idle;
                    self.power_demand_cache.mark_dirty(entity_id);
                    continue;
                }
                RocketLaunchPhase::Rising { ticks_remaining } => {
                    state.launch_phase = RocketLaunchPhase::Rising {
                        ticks_remaining: ticks_remaining - 1,
                    };
                    continue;
                }
                RocketLaunchPhase::Idle => {}
            }

            let Some(recipe) = rocket_silo_recipe(&self.world.prototypes, self.research) else {
                state.crafting_progress_ticks = 0;
                state.crafting_required_ticks = 0;
                continue;
            };
            // An assembler computes its required tick count when the player
            // picks a recipe. A silo has no such moment — nobody picks, and the
            // recipe appears when research lands — so the count is derived here
            // and the stored one is kept in step with it. Module effects still
            // arrive through `refresh_module_effects` like every other machine's;
            // what this covers is the recipe itself changing underfoot.
            let required_ticks = required_ticks_with_modules(
                recipe.crafting_time_ticks,
                state.crafting_speed_numerator,
                state.crafting_speed_denominator,
                state.modules.resolved_effects,
            );
            if state.crafting_required_ticks != required_ticks {
                state.crafting_progress_ticks = rescale_progress(
                    state.crafting_progress_ticks,
                    state.crafting_required_ticks,
                    required_ticks,
                );
                state.crafting_required_ticks = required_ticks;
            }
            let output_copies = state.modules.output_copies_due();

            let can_craft = profiler.measure(ProfilePhase::InventoryTransfers, || {
                ItemCraft {
                    input_inventory: &mut state.input_inventory,
                    products: CraftProducts::RocketParts {
                        completed: &mut state.parts_completed,
                        per_rocket: state.parts_per_rocket,
                    },
                }
                .can_craft(&self.world.prototypes, recipe, output_copies)
            });
            if !can_craft {
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
                advance_electric_progress(&mut state.crafting_progress_ticks, required_ticks),
                ProgressAdvance::Completed
            );
            self.pollution_emitters.mark_active(entity_id);
            if !completed {
                continue;
            }
            let bonus_copies = state.modules.complete_productive_cycle();
            debug_assert_eq!(output_copies, 1 + bonus_copies);

            let was_rocket_ready = state.rocket_ready();
            let parts_built = profiler.measure(ProfilePhase::InventoryTransfers, || {
                ItemCraft {
                    input_inventory: &mut state.input_inventory,
                    products: CraftProducts::RocketParts {
                        completed: &mut state.parts_completed,
                        per_rocket: state.parts_per_rocket,
                    },
                }
                .complete(&self.world.prototypes, recipe, output_copies)
            });
            if !was_rocket_ready && state.rocket_ready() {
                // The silo map is temporarily moved out of the entity store in
                // this pass, so record the endpoint transition directly.
                self.entities.changed_logistic_endpoints.insert(entity_id);
            }

            // Recipe slices borrow prototypes here, so record through the fields
            // instead of taking a mutable borrow of the whole tick context. Only
            // the parts that landed are counted: a productivity bonus lost to a
            // rocket that was already all but whole never existed.
            record_item_craft(
                &mut self.statistics,
                self.onboarding_progress,
                &self.base,
                recipe,
                parts_built,
            );
            self.power_demand_cache.mark_dirty(entity_id);
        }

        self.entities.rocket_silos = rocket_silos;
    }
}
