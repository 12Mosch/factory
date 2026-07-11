use super::progress::{ProgressAdvance, advance_burner_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_burner_mining_drills<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut burner_mining_drills = std::mem::take(&mut self.entities.burner_mining_drills);

        for (&entity_id, state) in &mut burner_mining_drills {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let prototype = &self.world.prototypes.entities[placed.prototype_id.index()];
            let Some(mining_drill) = prototype.mining_drill.as_ref() else {
                continue;
            };
            let output_target = drill_output_target(self.entities, &placed);
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                try_export_stored_drill_output_from_state(
                    self.entities,
                    self.transport,
                    state,
                    output_target,
                    &self.world.prototypes,
                );
            });

            let target = first_resource_in_mining_area_profiled(
                self.world,
                &placed.footprint,
                mining_drill,
                profiler,
            );
            let Some((target, resource_item)) = target else {
                state.resource_target = None;
                state.mining_progress_ticks = 0;
                continue;
            };

            let output_can_accept = profiler.measure(ProfilePhase::InventoryTransfers, || {
                drill_output_target_can_accept(
                    &self.world.prototypes,
                    self.entities,
                    output_target,
                    state.output_slot,
                    resource_item,
                    1,
                )
            });
            if !output_can_accept {
                state.resource_target = Some(target);
                continue;
            }

            state.resource_target = Some(target);
            let advance = advance_burner_progress(
                &self.world.prototypes,
                &mut state.energy,
                &mut state.mining_progress_ticks,
                state.mining_required_ticks,
                profiler,
            );
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
                insert_drill_output_from_state(
                    self.entities,
                    self.transport,
                    state,
                    output_target,
                    mined.resource_item,
                    mined.amount as u16,
                    &self.world.prototypes,
                );
            });
            self.record_item_produced(mined.resource_item, u64::from(mined.amount));
            let base = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
            if mined.resource_item == base.items.iron_ore {
                self.early_game_progress.iron_ore_drill_mined = self
                    .early_game_progress
                    .iron_ore_drill_mined
                    .saturating_add(u64::from(mined.amount));
                self.early_game_progress.changed();
            }
        }

        self.entities.burner_mining_drills = burner_mining_drills;
    }
}

fn insert_drill_output_from_state(
    entities: &mut EntityStore,
    transport: &mut TransportLaneCache,
    state: &mut BurnerMiningDrillState,
    output_target: DrillOutputTarget,
    item_id: ItemId,
    count: u16,
    catalog: &PrototypeCatalog,
) {
    match output_target {
        DrillOutputTarget::InternalSlot => {
            insert_output_item(&mut state.output_slot, item_id, count);
        }
        DrillOutputTarget::Inventory(entity_id) => {
            entities
                .entity_inventories
                .get_mut(&entity_id)
                .expect("validated output inventory should still exist")
                .insert(catalog, item_id, count)
                .expect("validated output inventory should accept drill product");
        }
        DrillOutputTarget::Belt(entity_id) => {
            let segment = entities
                .transport_belts
                .get_mut(&entity_id)
                .expect("validated output belt should still exist");
            let lane_index = belt_output_lane_index(segment, item_id)
                .expect("validated belt lane should accept");
            insert_lane_item_at_entry(&mut segment.lanes[lane_index], item_id, 0);
            transport.mark_active(TransportLaneKey::Belt {
                entity_id,
                lane_index,
            });
        }
        DrillOutputTarget::Splitter {
            entity_id,
            input_port,
        } => {
            let state = entities
                .splitters
                .get_mut(&entity_id)
                .expect("validated output splitter should still exist");
            let lane_index = splitter_output_lane_index(state, input_port, item_id)
                .expect("validated splitter lane should accept");
            insert_lane_item_at_entry(&mut state.input_lanes[input_port][lane_index], item_id, 0);
            transport.mark_active(TransportLaneKey::Splitter {
                entity_id,
                input_port,
                lane_index,
            });
        }
        DrillOutputTarget::Blocked => {
            unreachable!("blocked drill output is checked before mining")
        }
    }
}

fn try_export_stored_drill_output_from_state(
    entities: &mut EntityStore,
    transport: &mut TransportLaneCache,
    state: &mut BurnerMiningDrillState,
    output_target: DrillOutputTarget,
    catalog: &PrototypeCatalog,
) -> bool {
    if !matches!(
        output_target,
        DrillOutputTarget::Inventory(_)
            | DrillOutputTarget::Belt(_)
            | DrillOutputTarget::Splitter { .. }
    ) {
        return false;
    }

    let Some(stack) = state.output_slot else {
        return false;
    };

    if !drill_output_target_can_accept(catalog, entities, output_target, None, stack.item_id, 1) {
        return false;
    }

    insert_drill_output_from_state(
        entities,
        transport,
        state,
        output_target,
        stack.item_id,
        1,
        catalog,
    );
    remove_from_single_slot(&mut state.output_slot, stack.item_id, 1)
        .expect("stored drill output should still contain exported item");

    true
}
