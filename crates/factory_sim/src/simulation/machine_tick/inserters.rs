use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_inserters<P: TickProfiler>(&mut self, profiler: &mut P) {
        let mut inserters = std::mem::take(&mut self.entities.inserters);

        for (&entity_id, state) in &mut inserters {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(inserter) = prototype.inserter.as_ref() else {
                continue;
            };
            let pickup_ticks = inserter.pickup_ticks;
            let drop_ticks = inserter.drop_ticks;
            let (pickup_tile, drop_tile) = inserter_transfer_tiles_for_prototype(&placed, inserter);
            let Some(mut energy) = self.entities.inserter_energy.remove(&entity_id) else {
                continue;
            };

            let next_state = match *state {
                InserterState::WaitingForItem => {
                    let Some(item_id) = profiler.measure(ProfilePhase::InventoryTransfers, || {
                        peek_inserter_source_item(self.entities, pickup_tile)
                    }) else {
                        self.entities.inserter_energy.insert(entity_id, energy);
                        continue;
                    };
                    let item = ItemStack::new(&self.world.prototypes, item_id, 1)
                        .expect("a source item should exist in the prototype catalog");
                    if !profiler.measure(ProfilePhase::InventoryTransfers, || {
                        inserter_target_can_accept(
                            &self.world.prototypes,
                            self.research,
                            self.entities,
                            drop_tile,
                            item,
                        )
                    }) {
                        self.entities.inserter_energy.insert(entity_id, energy);
                        continue;
                    }
                    if !self.inserter_work_allowed(entity_id, &mut energy, profiler) {
                        self.entities.inserter_energy.insert(entity_id, energy);
                        continue;
                    }

                    InserterState::Picking {
                        ticks_left: pickup_ticks,
                    }
                }
                InserterState::Picking { ticks_left } => {
                    if !self.inserter_work_allowed(entity_id, &mut energy, profiler) {
                        InserterState::Picking { ticks_left }
                    } else if ticks_left > 1 {
                        InserterState::Picking {
                            ticks_left: ticks_left - 1,
                        }
                    } else if let Some(item_id) = profiler
                        .measure(ProfilePhase::InventoryTransfers, || {
                            peek_inserter_source_item(self.entities, pickup_tile)
                        })
                    {
                        let item = ItemStack::new(&self.world.prototypes, item_id, 1)
                            .expect("a source item should exist in the prototype catalog");
                        if !profiler.measure(ProfilePhase::InventoryTransfers, || {
                            inserter_target_can_accept(
                                &self.world.prototypes,
                                self.research,
                                self.entities,
                                drop_tile,
                                item,
                            )
                        }) {
                            InserterState::WaitingForItem
                        } else {
                            profiler
                                .measure(ProfilePhase::InventoryTransfers, || {
                                    try_take_inserter_source_item(
                                        &self.world.prototypes,
                                        self.entities,
                                        self.transport,
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
                                self.research,
                                self.entities,
                                drop_tile,
                                item,
                            )
                        });
                    if !target_can_accept
                        || !self.inserter_work_allowed(entity_id, &mut energy, profiler)
                    {
                        InserterState::Holding { item }
                    } else if profiler.measure(ProfilePhase::InventoryTransfers, || {
                        try_drop_inserter_item(
                            &self.world.prototypes,
                            self.research,
                            self.entities,
                            self.transport,
                            drop_tile,
                            item,
                        )
                    }) {
                        InserterState::Dropping {
                            ticks_left: drop_ticks,
                        }
                    } else {
                        InserterState::Holding { item }
                    }
                }
                InserterState::Dropping { ticks_left } => {
                    if !self.inserter_work_allowed(entity_id, &mut energy, profiler) {
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

            if matches!(
                (&*state, &next_state),
                (InserterState::Picking { .. }, InserterState::Holding { .. })
            ) && let Some(endpoint_id) = self
                .entities
                .occupancy
                .entity_at(pickup_tile.0, pickup_tile.1)
            {
                self.power_demand_cache.mark_dirty(endpoint_id);
            }
            if matches!(
                (&*state, &next_state),
                (
                    InserterState::Holding { .. },
                    InserterState::Dropping { .. }
                )
            ) && let Some(endpoint_id) =
                self.entities.occupancy.entity_at(drop_tile.0, drop_tile.1)
            {
                self.power_demand_cache.mark_dirty(endpoint_id);
            }
            if std::mem::discriminant(&*state) != std::mem::discriminant(&next_state) {
                self.power_demand_cache.mark_dirty(entity_id);
            }
            *state = next_state;
            self.entities.inserter_energy.insert(entity_id, energy);
        }

        self.entities.inserters = inserters;
    }

    fn inserter_work_allowed<P: TickProfiler>(
        &mut self,
        entity_id: EntityId,
        energy: &mut MachineEnergy,
        profiler: &mut P,
    ) -> bool {
        match energy {
            MachineEnergy::Electric => self.electric_work_allowed(entity_id),
            MachineEnergy::Burner(burner) => {
                let joules_per_tick = burner.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
                if burner.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    let consumed = profiler.measure(ProfilePhase::InventoryTransfers, || {
                        try_consume_fuel(&self.world.prototypes, burner)
                    });
                    if let Some(item_id) = consumed {
                        self.record_item_consumed(item_id, 1);
                    }
                    if consumed.is_none()
                        || burner.energy_remaining_joules + f64::EPSILON < joules_per_tick
                    {
                        return false;
                    }
                }
                burner.energy_remaining_joules -= joules_per_tick;
                self.mark_pollution_emitter_active(entity_id);
                true
            }
        }
    }
}
