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

            let next_state = match *state {
                InserterState::WaitingForItem => {
                    let Some(item_id) = profiler.measure(ProfilePhase::InventoryTransfers, || {
                        peek_inserter_source_item(self.entities, pickup_tile)
                    }) else {
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
                        continue;
                    }
                    if !self.electric_work_allowed(entity_id) {
                        continue;
                    }

                    InserterState::Picking {
                        ticks_left: pickup_ticks,
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
                    if !target_can_accept || !self.electric_work_allowed(entity_id) {
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

            *state = next_state;
        }

        self.entities.inserters = inserters;
    }
}
