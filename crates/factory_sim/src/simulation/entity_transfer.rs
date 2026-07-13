use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferOutcome {
    pub moved_quantity: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransferPlan {
    item_id: ItemId,
    moved_quantity: u16,
    stack_size: u16,
}

#[derive(Clone, Copy)]
enum TransferSource<'a> {
    Inventory {
        inventory: &'a Inventory,
        slot_index: usize,
    },
    SingleSlot {
        slot: Option<ItemStack>,
        slot_index: usize,
    },
}

#[derive(Clone, Copy)]
enum TransferDestination<'a> {
    Inventory(&'a Inventory),
    SingleSlot(Option<ItemStack>),
}

enum TransferSourceMut<'a> {
    Inventory {
        inventory: &'a mut Inventory,
        slot_index: usize,
    },
    SingleSlot(&'a mut Option<ItemStack>),
}

enum TransferDestinationMut<'a> {
    Inventory(&'a mut Inventory),
    SingleSlot(&'a mut Option<ItemStack>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferPlanError {
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    RejectedItem(ItemId),
    UnknownItem(ItemId),
    InsufficientSpace,
}

impl TransferSource<'_> {
    fn stack(self) -> Result<ItemStack, TransferPlanError> {
        match self {
            Self::Inventory {
                inventory,
                slot_index,
            } => inventory
                .slots()
                .get(slot_index)
                .ok_or(TransferPlanError::InvalidSlot { slot_index })?
                .ok_or(TransferPlanError::EmptySlot { slot_index }),
            Self::SingleSlot { slot, slot_index } => {
                slot.ok_or(TransferPlanError::EmptySlot { slot_index })
            }
        }
    }
}

impl TransferDestination<'_> {
    fn capacity(self, item_id: ItemId, stack_size: u16) -> u32 {
        match self {
            Self::Inventory(inventory) => inventory.insert_capacity(item_id, stack_size),
            Self::SingleSlot(None) => u32::from(stack_size),
            Self::SingleSlot(Some(existing)) if existing.item_id() == item_id => {
                u32::from(stack_size.saturating_sub(existing.count()))
            }
            Self::SingleSlot(Some(_)) => 0,
        }
    }
}

fn plan_transfer(
    catalog: &PrototypeCatalog,
    source: TransferSource<'_>,
    destination: TransferDestination<'_>,
    accepts_item: impl FnOnce(ItemId) -> bool,
) -> Result<TransferPlan, TransferPlanError> {
    let stack = source.stack()?;
    crate::inventory::validate_stack(catalog, stack)
        .map_err(|_| TransferPlanError::UnknownItem(stack.item_id()))?;

    if !accepts_item(stack.item_id()) {
        return Err(TransferPlanError::RejectedItem(stack.item_id()));
    }

    let stack_size = item_stack_size(catalog, stack.item_id())
        .ok_or(TransferPlanError::UnknownItem(stack.item_id()))?;
    let capacity = destination.capacity(stack.item_id(), stack_size);
    let moved_quantity = u32::from(stack.count()).min(capacity) as u16;
    if moved_quantity == 0 {
        return Err(TransferPlanError::InsufficientSpace);
    }

    Ok(TransferPlan {
        item_id: stack.item_id(),
        moved_quantity,
        stack_size,
    })
}

fn commit_transfer(
    plan: TransferPlan,
    source: TransferSourceMut<'_>,
    destination: TransferDestinationMut<'_>,
) -> TransferOutcome {
    match destination {
        TransferDestinationMut::Inventory(inventory) => {
            inventory.commit_prevalidated_insert(plan.item_id, plan.moved_quantity, plan.stack_size)
        }
        TransferDestinationMut::SingleSlot(slot) => {
            crate::inventory::commit_prevalidated_single_slot_insert(
                slot,
                plan.item_id,
                plan.moved_quantity,
                plan.stack_size,
            );
        }
    }

    match source {
        TransferSourceMut::Inventory {
            inventory,
            slot_index,
        } => inventory.commit_prevalidated_slot_removal(
            slot_index,
            plan.item_id,
            plan.moved_quantity,
        ),
        TransferSourceMut::SingleSlot(slot) => {
            crate::inventory::commit_prevalidated_single_slot_removal(
                slot,
                plan.item_id,
                plan.moved_quantity,
            );
        }
    }

    TransferOutcome {
        moved_quantity: plan.moved_quantity,
    }
}

fn map_plan_error<E: From<InventoryError>>(
    error: TransferPlanError,
    rejected_item: impl FnOnce(ItemId) -> E,
) -> E {
    match error {
        TransferPlanError::InvalidSlot { slot_index } => {
            InventoryError::InvalidSlot { slot_index }.into()
        }
        TransferPlanError::EmptySlot { slot_index } => {
            InventoryError::EmptySlot { slot_index }.into()
        }
        TransferPlanError::RejectedItem(item_id) => rejected_item(item_id),
        TransferPlanError::UnknownItem(item_id) => InventoryError::UnknownItem(item_id).into(),
        TransferPlanError::InsufficientSpace => InventoryError::InsufficientSpace.into(),
    }
}

pub fn transfer_container_slot(
    sim: &mut Simulation,
    entity_id: EntityId,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<TransferOutcome, SlotTransferError> {
    match panel {
        InventoryPanel::Player => {
            match entity_access::machine_kind(sim, entity_id) {
                Some(EntityKind::MiningDrill) => {
                    return player_slot_to_burner_drill_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::BurnerDrill);
                }
                Some(EntityKind::Furnace) => {
                    return player_slot_to_furnace(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Furnace);
                }
                Some(EntityKind::Boiler) => {
                    return player_slot_to_boiler_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Boiler);
                }
                Some(EntityKind::AssemblingMachine) => {
                    return player_slot_to_assembler_input(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Assembler);
                }
                _ => {}
            }
            player_slot_to_entity(sim, entity_id, slot_index)
        }
        InventoryPanel::Container => entity_slot_to_player(sim, entity_id, slot_index),
        InventoryPanel::BurnerFuel => {
            return burner_drill_fuel_to_player(sim, entity_id)
                .map_err(SlotTransferError::BurnerDrill);
        }
        InventoryPanel::BurnerOutput => {
            return burner_drill_output_to_player(sim, entity_id)
                .map_err(SlotTransferError::BurnerDrill);
        }
        InventoryPanel::FurnaceInput => {
            return furnace_input_to_player(sim, entity_id).map_err(SlotTransferError::Furnace);
        }
        InventoryPanel::FurnaceFuel => {
            return furnace_fuel_to_player(sim, entity_id).map_err(SlotTransferError::Furnace);
        }
        InventoryPanel::FurnaceOutput => {
            return furnace_output_to_player(sim, entity_id).map_err(SlotTransferError::Furnace);
        }
        InventoryPanel::BoilerFuel => {
            return boiler_fuel_to_player(sim, entity_id).map_err(SlotTransferError::Boiler);
        }
        InventoryPanel::AssemblerInput => {
            return assembler_input_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Assembler);
        }
        InventoryPanel::AssemblerOutput => {
            return assembler_output_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Assembler);
        }
    }
    .map_err(SlotTransferError::Transfer)
}

pub fn player_slot_to_entity(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, ContainerError> {
    let entity_inventory = EntityStore::entity_inventory(&sim.entities, entity_id)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: &sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(entity_inventory),
        |item_id| {
            (!sim.entities.labs.contains_key(&entity_id)
                || lab_can_accept_item(&sim.world.prototypes, item_id))
                && (!sim.entities.gun_turrets.contains_key(&entity_id)
                    || item_is_ammo(&sim.world.prototypes, item_id))
        },
    )
    .map_err(|error| map_plan_error(error, ContainerError::InvalidItem))?;

    let entity_inventory = EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)?;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: &mut sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestinationMut::Inventory(entity_inventory),
    ))
}

pub fn entity_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    entity_slot_index: usize,
) -> Result<TransferOutcome, ContainerError> {
    let entity_inventory = EntityStore::entity_inventory(&sim.entities, entity_id)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: entity_inventory,
            slot_index: entity_slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, ContainerError::InvalidItem))?;

    let entity_inventory = EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)?;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: entity_inventory,
            slot_index: entity_slot_index,
        },
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn player_slot_to_burner_drill_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, BurnerDrillError> {
    let fuel_slot = sim.entities.burner_drill_state(entity_id)?.energy.fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: &sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(fuel_slot),
        |item_id| burner_fuel_accepts_item(&sim.world.prototypes, item_id),
    )
    .map_err(|error| map_plan_error(error, BurnerDrillError::InvalidFuel))?;

    let fuel_slot = &mut sim
        .entities
        .burner_drill_state_mut(entity_id)?
        .energy
        .fuel_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: &mut sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestinationMut::SingleSlot(fuel_slot),
    ))
}

pub fn burner_drill_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, BurnerDrillError> {
    let fuel_slot = sim.entities.burner_drill_state(entity_id)?.energy.fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::SingleSlot {
            slot: fuel_slot,
            slot_index: BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, BurnerDrillError::InvalidFuel))?;

    let fuel_slot = &mut sim
        .entities
        .burner_drill_state_mut(entity_id)?
        .energy
        .fuel_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::SingleSlot(fuel_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn burner_drill_output_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, BurnerDrillError> {
    let output_slot = sim.entities.burner_drill_state(entity_id)?.output_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::SingleSlot {
            slot: output_slot,
            slot_index: BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, BurnerDrillError::InvalidFuel))?;

    let output_slot = &mut sim.entities.burner_drill_state_mut(entity_id)?.output_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::SingleSlot(output_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn player_slot_to_furnace_input(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, FurnaceError> {
    let input_slot = sim.entities.furnace_state(entity_id)?.input_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: &sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(input_slot),
        |item_id| furnace_input_accepts_item(&sim.world.prototypes, &sim.research, item_id),
    )
    .map_err(|error| map_plan_error(error, FurnaceError::InvalidInput))?;

    let input_slot = &mut sim.entities.furnace_state_mut(entity_id)?.input_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: &mut sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestinationMut::SingleSlot(input_slot),
    ))
}

pub fn player_slot_to_furnace_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, FurnaceError> {
    let fuel_slot = sim.entities.furnace_state(entity_id)?.energy.fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: &sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(fuel_slot),
        |item_id| burner_fuel_accepts_item(&sim.world.prototypes, item_id),
    )
    .map_err(|error| map_plan_error(error, FurnaceError::InvalidFuel))?;

    let fuel_slot = &mut sim.entities.furnace_state_mut(entity_id)?.energy.fuel_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: &mut sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestinationMut::SingleSlot(fuel_slot),
    ))
}

pub fn furnace_input_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, FurnaceError> {
    let input_slot = sim.entities.furnace_state(entity_id)?.input_slot;
    transfer_furnace_slot_to_player(
        sim,
        entity_id,
        input_slot,
        FURNACE_INPUT_SLOT_INDEX,
        |state| &mut state.input_slot,
    )
}

pub fn furnace_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, FurnaceError> {
    let fuel_slot = sim.entities.furnace_state(entity_id)?.energy.fuel_slot;
    transfer_furnace_slot_to_player(
        sim,
        entity_id,
        fuel_slot,
        FURNACE_FUEL_SLOT_INDEX,
        |state| &mut state.energy.fuel_slot,
    )
}

pub fn furnace_output_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, FurnaceError> {
    let output_slot = sim.entities.furnace_state(entity_id)?.output_slot;
    transfer_furnace_slot_to_player(
        sim,
        entity_id,
        output_slot,
        FURNACE_OUTPUT_SLOT_INDEX,
        |state| &mut state.output_slot,
    )
}

fn transfer_furnace_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot: Option<ItemStack>,
    slot_index: usize,
    slot_mut: impl FnOnce(&mut FurnaceState) -> &mut Option<ItemStack>,
) -> Result<TransferOutcome, FurnaceError> {
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::SingleSlot { slot, slot_index },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, FurnaceError::InvalidInput))?;

    let source = slot_mut(sim.entities.furnace_state_mut(entity_id)?);
    Ok(commit_transfer(
        plan,
        TransferSourceMut::SingleSlot(source),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn player_slot_to_boiler_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, BoilerError> {
    let fuel_slot = sim.entities.boiler_state(entity_id)?.energy.fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: &sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(fuel_slot),
        |item_id| burner_fuel_accepts_item(&sim.world.prototypes, item_id),
    )
    .map_err(|error| map_plan_error(error, BoilerError::InvalidFuel))?;

    let fuel_slot = &mut sim.entities.boiler_state_mut(entity_id)?.energy.fuel_slot;
    let outcome = commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: &mut sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestinationMut::SingleSlot(fuel_slot),
    );
    sim.invalidate_power_dynamic_state();
    Ok(outcome)
}

pub fn boiler_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, BoilerError> {
    let fuel_slot = sim.entities.boiler_state(entity_id)?.energy.fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::SingleSlot {
            slot: fuel_slot,
            slot_index: BOILER_FUEL_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, BoilerError::InvalidFuel))?;

    let fuel_slot = &mut sim.entities.boiler_state_mut(entity_id)?.energy.fuel_slot;
    let outcome = commit_transfer(
        plan,
        TransferSourceMut::SingleSlot(fuel_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    );
    sim.invalidate_power_dynamic_state();
    Ok(outcome)
}

pub fn player_slot_to_assembler_input(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, AssemblerError> {
    let machine_category =
        assembler_machine_category(&sim.world.prototypes, &sim.entities, entity_id);
    let state = sim.entities.assembler_state(entity_id)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: &sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(&state.input_inventory),
        |item_id| {
            assembler_input_accepts_item(
                &sim.world.prototypes,
                &sim.research,
                machine_category,
                state,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, AssemblerError::InvalidInput))?;

    let input_inventory = &mut sim.entities.assembler_state_mut(entity_id)?.input_inventory;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: &mut sim.player_inventory,
            slot_index: player_slot_index,
        },
        TransferDestinationMut::Inventory(input_inventory),
    ))
}

pub fn assembler_input_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, AssemblerError> {
    transfer_assembler_slot_to_player(
        sim,
        entity_id,
        slot_index,
        |state| &state.input_inventory,
        |state| &mut state.input_inventory,
    )
}

pub fn assembler_output_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, AssemblerError> {
    transfer_assembler_slot_to_player(
        sim,
        entity_id,
        slot_index,
        |state| &state.output_inventory,
        |state| &mut state.output_inventory,
    )
}

fn transfer_assembler_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
    inventory: impl FnOnce(&AssemblingMachineState) -> &Inventory,
    inventory_mut: impl FnOnce(&mut AssemblingMachineState) -> &mut Inventory,
) -> Result<TransferOutcome, AssemblerError> {
    let source_inventory = inventory(sim.entities.assembler_state(entity_id)?);
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource::Inventory {
            inventory: source_inventory,
            slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, AssemblerError::InvalidInput))?;

    let source_inventory = inventory_mut(sim.entities.assembler_state_mut(entity_id)?);
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Inventory {
            inventory: source_inventory,
            slot_index,
        },
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

fn player_slot_to_furnace(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, FurnaceError> {
    let stack = sim
        .player_inventory()
        .slots()
        .get(slot_index)
        .ok_or(FurnaceError::InvalidSlot { slot_index })?
        .ok_or(FurnaceError::EmptySlot { slot_index })?;
    let is_fuel = burner_fuel_accepts_item(sim.catalog(), stack.item_id());

    if is_fuel {
        player_slot_to_furnace_fuel(sim, entity_id, slot_index)
    } else {
        player_slot_to_furnace_input(sim, entity_id, slot_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_partially_moves_between_exact_inventory_slots() {
        let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
        let item_id = factory_data::item_id_by_name(&catalog, "iron_plate");
        let mut source = Inventory::from_slots(
            &catalog,
            vec![
                Some(ItemStack::new(&catalog, item_id, 40).unwrap()),
                Some(ItemStack::new(&catalog, item_id, 60).unwrap()),
            ],
        )
        .unwrap();
        let mut destination = Inventory::from_slots(
            &catalog,
            vec![Some(ItemStack::new(&catalog, item_id, 75).unwrap())],
        )
        .unwrap();

        let plan = plan_transfer(
            &catalog,
            TransferSource::Inventory {
                inventory: &source,
                slot_index: 0,
            },
            TransferDestination::Inventory(&destination),
            |_| true,
        )
        .unwrap();
        let outcome = commit_transfer(
            plan,
            TransferSourceMut::Inventory {
                inventory: &mut source,
                slot_index: 0,
            },
            TransferDestinationMut::Inventory(&mut destination),
        );

        assert_eq!(outcome, TransferOutcome { moved_quantity: 25 });
        assert_eq!(source.slot(0).unwrap().count(), 15);
        assert_eq!(source.slot(1).unwrap().count(), 60);
        assert_eq!(destination.slot(0).unwrap().count(), 100);
    }

    #[test]
    fn primitive_planning_errors_do_not_mutate_either_endpoint() {
        let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
        let iron_plate = factory_data::item_id_by_name(&catalog, "iron_plate");
        let coal = factory_data::item_id_by_name(&catalog, "coal");
        let source = Inventory::from_slots(
            &catalog,
            vec![Some(ItemStack::new(&catalog, iron_plate, 10).unwrap())],
        )
        .unwrap();
        let destination = Inventory::from_slots(
            &catalog,
            vec![Some(ItemStack::new(&catalog, coal, 100).unwrap())],
        )
        .unwrap();
        let source_before = source.clone();
        let destination_before = destination.clone();

        assert_eq!(
            plan_transfer(
                &catalog,
                TransferSource::Inventory {
                    inventory: &source,
                    slot_index: 0,
                },
                TransferDestination::Inventory(&destination),
                |_| true,
            ),
            Err(TransferPlanError::InsufficientSpace)
        );
        assert_eq!(source, source_before);
        assert_eq!(destination, destination_before);
    }

    #[test]
    fn unknown_source_items_fail_planning_without_mutation() {
        let source_catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
        let item_id = source_catalog
            .items
            .last()
            .expect("base prototypes should contain items")
            .id;
        let source = Inventory::from_slots(
            &source_catalog,
            vec![Some(ItemStack::new(&source_catalog, item_id, 1).unwrap())],
        )
        .unwrap();
        let destination = Inventory::with_slot_count(1);
        let source_before = source.clone();
        let destination_before = destination.clone();
        let mut destination_catalog = source_catalog;
        destination_catalog.items.pop();

        assert_eq!(
            plan_transfer(
                &destination_catalog,
                TransferSource::Inventory {
                    inventory: &source,
                    slot_index: 0,
                },
                TransferDestination::Inventory(&destination),
                |_| true,
            ),
            Err(TransferPlanError::UnknownItem(item_id))
        );
        assert_eq!(source, source_before);
        assert_eq!(destination, destination_before);
    }
}
