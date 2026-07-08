use super::progress::{ProgressAdvance, advance_burner_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_burner_mining_drills<P: TickProfiler>(&mut self, profiler: &mut P) {
        let drill_ids = self
            .entities
            .burner_mining_drills
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in drill_ids {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let prototype = &self.world.prototypes.entities[placed.prototype_id.index()];
            let Some(mining_drill) = prototype.mining_drill.as_ref() else {
                continue;
            };
            let output_target = drill_output_target(self.entities, &placed);
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                try_export_stored_drill_output(
                    self.entities,
                    entity_id,
                    output_target,
                    &self.world.prototypes,
                )
            });

            let target = first_resource_in_mining_area_profiled(
                self.world,
                &placed.footprint,
                mining_drill,
                profiler,
            );
            let Some((target, resource_item)) = target else {
                if let Ok(state) = self.entities.burner_drill_state_mut(entity_id) {
                    state.resource_target = None;
                    state.mining_progress_ticks = 0;
                }
                continue;
            };

            let output_can_accept =
                self.entities
                    .burner_drill_state(entity_id)
                    .is_ok_and(|state| {
                        profiler.measure(ProfilePhase::InventoryTransfers, || {
                            drill_output_target_can_accept(
                                &self.world.prototypes,
                                self.entities,
                                output_target,
                                state.output_slot,
                                resource_item,
                                1,
                            )
                        })
                    });
            if !output_can_accept {
                if let Ok(state) = self.entities.burner_drill_state_mut(entity_id) {
                    state.resource_target = Some(target);
                }
                continue;
            }

            let advance = {
                let state = self
                    .entities
                    .burner_drill_state_mut(entity_id)
                    .expect("burner drill id came from burner drill state map");
                state.resource_target = Some(target);
                advance_burner_progress(
                    &self.world.prototypes,
                    &mut state.energy,
                    &mut state.mining_progress_ticks,
                    state.mining_required_ticks,
                    profiler,
                )
            };
            if let Some(item_id) = advance.consumed_fuel {
                self.record_item_consumed(item_id, 1);
            }

            if !matches!(advance.result, ProgressAdvance::Completed) {
                continue;
            }

            let mined = self
                .world
                .mine_resource_at_profiled(target.x, target.y, 1, profiler)
                .expect("selected drill target should contain a resource");
            debug_assert_eq!(mined.resource_item, resource_item);
            debug_assert_eq!(mined.amount, 1);
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                insert_drill_output(
                    self.entities,
                    entity_id,
                    output_target,
                    mined.resource_item,
                    mined.amount as u16,
                    &self.world.prototypes,
                );
            });
            self.record_item_produced(mined.resource_item, u64::from(mined.amount));
        }
    }
}
