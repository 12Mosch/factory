use super::*;

impl MachineTickContext<'_> {
    pub(super) fn advance_inserters<P: TickProfiler>(&mut self, profiler: &mut P) {
        let inserter_ids = self.entities.inserters.keys().copied().collect::<Vec<_>>();

        for entity_id in inserter_ids {
            let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
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
                        peek_inserter_source_item(self.entities, pickup_tile)
                    }) else {
                        continue;
                    };
                    let item = ItemStack { item_id, count: 1 };
                    if !profiler.measure(ProfilePhase::InventoryTransfers, || {
                        inserter_target_can_accept(
                            &self.world.prototypes,
                            self.research,
                            self.entities,
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
                            peek_inserter_source_item(self.entities, pickup_tile)
                        })
                    {
                        let item = ItemStack { item_id, count: 1 };
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
                                        self.entities,
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
                    if !target_can_accept || !self.electric_work_allowed(entity_id) {
                        InserterState::Holding { item }
                    } else if profiler.measure(ProfilePhase::InventoryTransfers, || {
                        try_drop_inserter_item(
                            &self.world.prototypes,
                            self.research,
                            self.entities,
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
