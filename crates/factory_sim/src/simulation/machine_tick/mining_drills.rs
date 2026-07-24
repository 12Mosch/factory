use super::progress::{MachineEnergyContext, ProgressAdvance, advance_machine_progress};
use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_mining_drills<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut mining_drills = std::mem::take(&mut self.entities.mining_drills);

        for (&entity_id, state) in &mut mining_drills {
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

            let output_copies = state.modules.output_copies_due();
            let output_can_accept = profiler.measure(ProfilePhase::InventoryTransfers, || {
                drill_productivity_output_can_fit(
                    &self.world.prototypes,
                    self.entities,
                    output_target,
                    state.output_slot,
                    resource_item,
                    output_copies,
                )
            });
            if !output_can_accept {
                state.resource_target = Some(target);
                continue;
            }

            state.resource_target = Some(target);
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
                &mut state.mining_progress_ticks,
                state.mining_required_ticks,
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

            let mined = self
                .world
                .mine_resource_at_profiled(target.x, target.y, 1, profiler)
                .expect("selected drill target should contain a resource");
            debug_assert_eq!(mined.resource_item, resource_item);
            debug_assert_eq!(mined.amount, 1);
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                insert_productive_drill_output(
                    self.entities,
                    self.transport,
                    state,
                    output_target,
                    mined.resource_item,
                    output_copies as u16,
                    &self.world.prototypes,
                );
            });
            self.record_item_produced(mined.resource_item, output_copies);
            if mined.resource_item == self.base.items.iron_ore {
                self.onboarding_progress
                    .record_counter(|progress| &mut progress.iron_ore_drill_mined, output_copies);
            }
        }

        self.entities.mining_drills = mining_drills;
    }
}

fn insert_productive_drill_output(
    entities: &mut EntityStore,
    transport: &mut TransportLaneCache,
    state: &mut MiningDrillState,
    output_target: DrillOutputTarget,
    item_id: ItemId,
    copies: u16,
    catalog: &PrototypeCatalog,
) {
    match output_target {
        DrillOutputTarget::InternalSlot | DrillOutputTarget::Inventory(_) => {
            insert_drill_output_from_state(
                entities,
                transport,
                state,
                output_target,
                item_id,
                copies,
                catalog,
            );
        }
        DrillOutputTarget::Belt(_) | DrillOutputTarget::Splitter { .. } => {
            insert_drill_output_from_state(
                entities,
                transport,
                state,
                output_target,
                item_id,
                1,
                catalog,
            );
            let surplus = copies.saturating_sub(1);
            if surplus > 0 {
                state
                    .output_slot
                    .insert(catalog, item_id, surplus)
                    .expect("productive drill surplus capacity was checked");
            }
        }
        DrillOutputTarget::Blocked => {
            unreachable!("blocked drill output is checked before mining");
        }
    }
}

fn insert_drill_output_from_state(
    entities: &mut EntityStore,
    transport: &mut TransportLaneCache,
    state: &mut MiningDrillState,
    output_target: DrillOutputTarget,
    item_id: ItemId,
    count: u16,
    catalog: &PrototypeCatalog,
) {
    match output_target {
        DrillOutputTarget::InternalSlot => {
            state
                .output_slot
                .insert(catalog, item_id, count)
                .expect("the checked drill output slot should accept the product");
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
            let item = BeltItem {
                id: transport.allocate_item_id(),
                item_id,
                position_subtile: 0,
            };
            insert_lane_item_at_entry(&mut segment.lanes[lane_index], item);
            transport.mark_items_changed(entity_id);
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
            let item = BeltItem {
                id: transport.allocate_item_id(),
                item_id,
                position_subtile: 0,
            };
            insert_lane_item_at_entry(&mut state.input_lanes[input_port][lane_index], item);
            transport.mark_items_changed(entity_id);
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
    state: &mut MiningDrillState,
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

    let Some(stack) = state.output_slot.stack() else {
        return false;
    };

    if !drill_output_target_can_accept(
        catalog,
        entities,
        output_target,
        ItemSlot::default(),
        stack.item_id(),
        1,
    ) {
        return false;
    }

    insert_drill_output_from_state(
        entities,
        transport,
        state,
        output_target,
        stack.item_id(),
        1,
        catalog,
    );
    state
        .output_slot
        .remove(stack.item_id(), 1)
        .expect("stored drill output should still contain exported item");

    true
}
