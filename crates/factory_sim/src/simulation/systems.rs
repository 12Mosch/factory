use super::*;

impl Simulation {
    pub fn advance_transport_belts(&mut self) {
        let mut lane_keys = self
            .entities
            .transport_belts
            .keys()
            .flat_map(|entity_id| {
                [
                    TransportLaneKey::Belt {
                        entity_id: *entity_id,
                        lane_index: 0,
                    },
                    TransportLaneKey::Belt {
                        entity_id: *entity_id,
                        lane_index: 1,
                    },
                ]
            })
            .collect::<Vec<_>>();
        lane_keys.extend(self.entities.splitters.keys().flat_map(|entity_id| {
            [
                TransportLaneKey::Splitter {
                    entity_id: *entity_id,
                    input_port: 0,
                    lane_index: 0,
                },
                TransportLaneKey::Splitter {
                    entity_id: *entity_id,
                    input_port: 0,
                    lane_index: 1,
                },
                TransportLaneKey::Splitter {
                    entity_id: *entity_id,
                    input_port: 1,
                    lane_index: 0,
                },
                TransportLaneKey::Splitter {
                    entity_id: *entity_id,
                    input_port: 1,
                    lane_index: 1,
                },
            ]
        }));
        let mut advancement = TransportBeltAdvancement::new(&mut self.entities);

        for key in lane_keys {
            advancement.process_lane(key);
        }
    }

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
            let output_target = drill_output_target(&self.entities, &placed);
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                try_export_stored_drill_output(
                    &mut self.entities,
                    entity_id,
                    output_target,
                    &self.world.prototypes,
                )
            });

            let target = first_resource_in_mining_area_profiled(
                &self.world,
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
                                &self.entities,
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

            let (ready, completed, consumed_fuel) = {
                let state = self
                    .entities
                    .burner_drill_state_mut(entity_id)
                    .expect("burner drill id came from burner drill state map");
                let mut consumed_fuel = None;
                state.resource_target = Some(target);
                let joules_per_tick =
                    state.energy.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    consumed_fuel = profiler.measure(ProfilePhase::InventoryTransfers, || {
                        try_consume_fuel(&self.world.prototypes, &mut state.energy)
                    });
                    if consumed_fuel.is_none()
                        || state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick
                    {
                        (false, false, consumed_fuel)
                    } else {
                        state.energy.energy_remaining_joules -= joules_per_tick;
                        state.mining_progress_ticks += 1;

                        if state.mining_progress_ticks < state.mining_required_ticks {
                            (true, false, consumed_fuel)
                        } else {
                            state.mining_progress_ticks = 0;
                            (true, true, consumed_fuel)
                        }
                    }
                } else {
                    state.energy.energy_remaining_joules -= joules_per_tick;
                    state.mining_progress_ticks += 1;

                    if state.mining_progress_ticks < state.mining_required_ticks {
                        (true, false, consumed_fuel)
                    } else {
                        state.mining_progress_ticks = 0;
                        (true, true, consumed_fuel)
                    }
                }
            };
            if let Some(item_id) = consumed_fuel {
                self.record_item_consumed(item_id, 1);
            }

            if !ready || !completed {
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
                    &mut self.entities,
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

    pub(super) fn advance_furnaces<P: TickProfiler>(&mut self, profiler: &mut P) {
        let furnace_ids = self.entities.furnaces.keys().copied().collect::<Vec<_>>();

        for entity_id in furnace_ids {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some((recipe_id, required_ticks, ingredient, product)) = self
                .entities
                .furnace_state(entity_id)
                .ok()
                .and_then(|state| {
                    furnace_work_selection(&self.world.prototypes, &self.research, state.input_slot)
                })
            else {
                if let Ok(state) = self.entities.furnace_state_mut(entity_id) {
                    state.active_recipe = None;
                    state.crafting_progress_ticks = 0;
                    state.crafting_required_ticks = 0;
                }
                continue;
            };

            let output_can_accept = self.entities.furnace_state(entity_id).is_ok_and(|state| {
                profiler.measure(ProfilePhase::InventoryTransfers, || {
                    output_slot_can_accept(
                        &self.world.prototypes,
                        state.output_slot,
                        product.item,
                        product.amount,
                    )
                })
            });
            if !output_can_accept {
                if let Ok(state) = self.entities.furnace_state_mut(entity_id) {
                    if state.active_recipe != Some(recipe_id) {
                        state.crafting_progress_ticks = 0;
                    }
                    state.active_recipe = Some(recipe_id);
                    state.crafting_required_ticks = required_ticks;
                }
                continue;
            }

            let (ready, completed, consumed_fuel) = {
                let state = self
                    .entities
                    .furnace_state_mut(entity_id)
                    .expect("furnace id came from furnace state map");
                let mut consumed_fuel = None;
                if state.active_recipe != Some(recipe_id) {
                    state.active_recipe = Some(recipe_id);
                    state.crafting_progress_ticks = 0;
                    state.crafting_required_ticks = required_ticks;
                }

                let joules_per_tick =
                    state.energy.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    consumed_fuel = profiler.measure(ProfilePhase::InventoryTransfers, || {
                        try_consume_fuel(&self.world.prototypes, &mut state.energy)
                    });
                    if consumed_fuel.is_none()
                        || state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick
                    {
                        (false, false, consumed_fuel)
                    } else {
                        state.energy.energy_remaining_joules -= joules_per_tick;
                        state.crafting_progress_ticks += 1;

                        if state.crafting_progress_ticks < required_ticks {
                            (true, false, consumed_fuel)
                        } else {
                            state.crafting_progress_ticks = 0;
                            (true, true, consumed_fuel)
                        }
                    }
                } else {
                    state.energy.energy_remaining_joules -= joules_per_tick;
                    state.crafting_progress_ticks += 1;

                    if state.crafting_progress_ticks < required_ticks {
                        (true, false, consumed_fuel)
                    } else {
                        state.crafting_progress_ticks = 0;
                        (true, true, consumed_fuel)
                    }
                }
            };
            if let Some(item_id) = consumed_fuel {
                self.record_item_consumed(item_id, 1);
            }

            if !ready || !completed {
                continue;
            }

            let state = self
                .entities
                .furnace_state_mut(entity_id)
                .expect("furnace id came from furnace state map");
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                remove_from_single_slot(&mut state.input_slot, ingredient.item, ingredient.amount)
                    .expect("selected furnace input should still contain ingredient");
                insert_output_item(&mut state.output_slot, product.item, product.amount);
            });
            self.record_item_consumed(ingredient.item, u64::from(ingredient.amount));
            self.record_item_produced(product.item, u64::from(product.amount));
        }
    }

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
                        selected_assembler_recipe(&self.world.prototypes, &self.research, state)?;
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
                state.crafting_progress_ticks += 1;

                if state.crafting_progress_ticks < required_ticks {
                    false
                } else {
                    state.crafting_progress_ticks = 0;
                    true
                }
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

    pub(super) fn advance_labs<P: TickProfiler>(&mut self, profiler: &mut P) {
        let lab_ids = self.entities.labs.keys().copied().collect::<Vec<_>>();

        for entity_id in lab_ids {
            if self.entities.placed_entity(entity_id).is_none() {
                continue;
            }

            let Some(technology_id) = self.research.active else {
                if let Ok(state) = self.entities.lab_state_mut(entity_id) {
                    state.active_technology = None;
                    state.progress_ticks = 0;
                    state.required_ticks = 0;
                }
                continue;
            };

            let Some(technology) = self
                .world
                .prototypes
                .technologies
                .get(technology_id.index())
                .filter(|technology| technology.id == technology_id)
            else {
                if let Ok(state) = self.entities.lab_state_mut(entity_id) {
                    state.active_technology = None;
                    state.progress_ticks = 0;
                    state.required_ticks = 0;
                }
                continue;
            };
            let required_ticks = technology.research_time_ticks;
            let science_packs = technology.science_packs.clone();

            let can_work = {
                let state = self
                    .entities
                    .lab_state_mut(entity_id)
                    .expect("lab id came from lab state map");
                if state.active_technology != Some(technology_id) {
                    state.active_technology = Some(technology_id);
                    state.progress_ticks = 0;
                }
                state.required_ticks = required_ticks;
                profiler.measure(ProfilePhase::InventoryTransfers, || {
                    lab_has_science_packs(&state.inventory, &science_packs)
                })
            };
            if !can_work {
                continue;
            }
            if !self.electric_work_allowed(entity_id) {
                continue;
            }

            let completed = {
                let state = self
                    .entities
                    .lab_state_mut(entity_id)
                    .expect("lab id came from lab state map");
                state.progress_ticks += 1;
                if state.progress_ticks < required_ticks {
                    false
                } else {
                    state.progress_ticks = 0;
                    true
                }
            };
            if !completed {
                continue;
            }

            let state = self
                .entities
                .lab_state_mut(entity_id)
                .expect("lab id came from lab state map");
            profiler.measure(ProfilePhase::InventoryTransfers, || {
                for science_pack in &science_packs {
                    state
                        .inventory
                        .remove(science_pack.item, science_pack.amount)
                        .expect("lab checked science packs before completion");
                }
            });
            for science_pack in &science_packs {
                self.record_item_consumed(science_pack.item, u64::from(science_pack.amount));
            }
            self.add_research_units(1)
                .expect("lab completion should have active research");
        }
    }

    pub(super) fn advance_inserters<P: TickProfiler>(&mut self, profiler: &mut P) {
        let inserter_ids = self.entities.inserters.keys().copied().collect::<Vec<_>>();

        for entity_id in inserter_ids {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let Some(prototype) = self
                .world
                .prototypes
                .entities
                .get(placed.prototype_id.index())
                .filter(|prototype| prototype.id == placed.prototype_id)
            else {
                continue;
            };
            let Some(inserter) = prototype.inserter.as_ref().cloned() else {
                continue;
            };
            let Ok(state) = self.entities.inserter_state(entity_id).cloned() else {
                continue;
            };
            let (pickup_tile, drop_tile) =
                inserter_transfer_tiles_for_prototype(&placed, &inserter);

            let next_state = match state {
                InserterState::WaitingForItem => {
                    let Some(item_id) = profiler.measure(ProfilePhase::InventoryTransfers, || {
                        peek_inserter_source_item(&self.entities, pickup_tile)
                    }) else {
                        continue;
                    };
                    let item = ItemStack { item_id, count: 1 };
                    if !profiler.measure(ProfilePhase::InventoryTransfers, || {
                        inserter_target_can_accept(
                            &self.world.prototypes,
                            &self.research,
                            &self.entities,
                            drop_tile,
                            item,
                        )
                    }) {
                        continue;
                    }
                    if !self.electric_work_allowed(entity_id) {
                        continue;
                    }

                    InserterState::Picking {
                        ticks_left: inserter.pickup_ticks,
                    }
                }
                InserterState::Picking { ticks_left } => {
                    if !self.electric_work_allowed(entity_id) {
                        InserterState::Picking { ticks_left }
                    } else if ticks_left > 1 {
                        InserterState::Picking {
                            ticks_left: ticks_left - 1,
                        }
                    } else if let Some(item_id) = profiler
                        .measure(ProfilePhase::InventoryTransfers, || {
                            peek_inserter_source_item(&self.entities, pickup_tile)
                        })
                    {
                        let item = ItemStack { item_id, count: 1 };
                        if !profiler.measure(ProfilePhase::InventoryTransfers, || {
                            inserter_target_can_accept(
                                &self.world.prototypes,
                                &self.research,
                                &self.entities,
                                drop_tile,
                                item,
                            )
                        }) {
                            InserterState::WaitingForItem
                        } else {
                            profiler
                                .measure(ProfilePhase::InventoryTransfers, || {
                                    try_take_inserter_source_item(
                                        &mut self.entities,
                                        pickup_tile,
                                        item_id,
                                    )
                                })
                                .map_or(InserterState::WaitingForItem, |item| {
                                    InserterState::Holding { item }
                                })
                        }
                    } else {
                        InserterState::WaitingForItem
                    }
                }
                InserterState::Holding { item } => {
                    let target_can_accept =
                        profiler.measure(ProfilePhase::InventoryTransfers, || {
                            inserter_target_can_accept(
                                &self.world.prototypes,
                                &self.research,
                                &self.entities,
                                drop_tile,
                                item,
                            )
                        });
                    if !target_can_accept || !self.electric_work_allowed(entity_id) {
                        InserterState::Holding { item }
                    } else if profiler.measure(ProfilePhase::InventoryTransfers, || {
                        try_drop_inserter_item(
                            &self.world.prototypes,
                            &self.research,
                            &mut self.entities,
                            drop_tile,
                            item,
                        )
                    }) {
                        InserterState::Dropping {
                            ticks_left: inserter.drop_ticks,
                        }
                    } else {
                        InserterState::Holding { item }
                    }
                }
                InserterState::Dropping { ticks_left } => {
                    if !self.electric_work_allowed(entity_id) {
                        InserterState::Dropping { ticks_left }
                    } else if ticks_left > 1 {
                        InserterState::Dropping {
                            ticks_left: ticks_left - 1,
                        }
                    } else {
                        InserterState::WaitingForItem
                    }
                }
            };

            if let Ok(state) = self.entities.inserter_state_mut(entity_id) {
                *state = next_state;
            }
        }
    }
}
