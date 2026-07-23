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
struct TransferSource<'a> {
    slot: Option<&'a ItemSlot>,
    slot_index: usize,
}

#[derive(Clone, Copy)]
enum TransferDestination<'a> {
    Inventory(&'a Inventory),
    SingleSlot(&'a ItemSlot),
}

enum TransferSourceMut<'a> {
    Slot(&'a mut ItemSlot),
}

enum TransferDestinationMut<'a> {
    Inventory(&'a mut Inventory),
    SingleSlot(&'a mut ItemSlot),
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
        self.slot
            .ok_or(TransferPlanError::InvalidSlot {
                slot_index: self.slot_index,
            })?
            .stack()
            .ok_or(TransferPlanError::EmptySlot {
                slot_index: self.slot_index,
            })
    }
}

impl TransferDestination<'_> {
    fn capacity(self, item_id: ItemId, stack_size: u16) -> u32 {
        match self {
            Self::Inventory(inventory) => inventory.insert_capacity(item_id, stack_size),
            Self::SingleSlot(slot) => u32::from(slot.capacity_for(item_id, stack_size)),
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
            slot.commit_prevalidated_insert(plan.item_id, plan.moved_quantity, plan.stack_size);
        }
    }

    match source {
        TransferSourceMut::Slot(slot) => {
            slot.commit_prevalidated_removal(plan.item_id, plan.moved_quantity);
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
                    return player_slot_to_mining_drill_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::MiningDrill);
                }
                Some(EntityKind::Furnace) => {
                    return player_slot_to_furnace(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Furnace);
                }
                Some(EntityKind::Boiler) => {
                    return player_slot_to_boiler_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Boiler);
                }
                Some(EntityKind::Inserter) => {
                    return player_slot_to_inserter_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Inserter);
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
            return mining_drill_fuel_to_player(sim, entity_id)
                .map_err(SlotTransferError::MiningDrill);
        }
        InventoryPanel::BurnerOutput => {
            return mining_drill_output_to_player(sim, entity_id)
                .map_err(SlotTransferError::MiningDrill);
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
        InventoryPanel::InserterFuel => {
            return inserter_fuel_to_player(sim, entity_id).map_err(SlotTransferError::Inserter);
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
    let policy = inventory_policy_for_entity(&sim.entities, entity_id);
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(entity_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                policy,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, ContainerError::InvalidItem))?;

    let outcome = {
        let entity_inventory = EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)?;
        commit_transfer(
            plan,
            TransferSourceMut::Slot(
                sim.player_inventory
                    .item_slot_mut(player_slot_index)
                    .expect("a planned player source slot remains in bounds"),
            ),
            TransferDestinationMut::Inventory(entity_inventory),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
}

pub fn entity_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    entity_slot_index: usize,
) -> Result<TransferOutcome, ContainerError> {
    let entity_inventory = EntityStore::entity_inventory(&sim.entities, entity_id)?;
    let policy = inventory_policy_for_entity(&sim.entities, entity_id);
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: entity_inventory.item_slot(entity_slot_index),
            slot_index: entity_slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                policy,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, ContainerError::InvalidItem))?;

    let outcome = {
        let entity_inventory = EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)?;
        commit_transfer(
            plan,
            TransferSourceMut::Slot(
                entity_inventory
                    .item_slot_mut(entity_slot_index)
                    .expect("a planned entity source slot remains in bounds"),
            ),
            TransferDestinationMut::Inventory(&mut sim.player_inventory),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
}

pub fn player_slot_to_mining_drill_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, MiningDrillError> {
    let fuel_slot = sim
        .entities
        .mining_drill_state(entity_id)?
        .energy
        .fuel_slot()
        .ok_or(MiningDrillError::NoFuelSlot)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(&fuel_slot),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, MiningDrillError::InvalidFuel))?;

    let fuel_slot = sim
        .entities
        .mining_drill_state_mut(entity_id)?
        .energy
        .fuel_slot_mut()
        .expect("a planned drill fuel transfer targets a burner drill");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
        TransferDestinationMut::SingleSlot(fuel_slot),
    ))
}

pub fn mining_drill_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, MiningDrillError> {
    let fuel_slot = sim
        .entities
        .mining_drill_state(entity_id)?
        .energy
        .fuel_slot()
        .ok_or(MiningDrillError::NoFuelSlot)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&fuel_slot),
            slot_index: MINING_DRILL_FUEL_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, MiningDrillError::InvalidFuel))?;

    let fuel_slot = sim
        .entities
        .mining_drill_state_mut(entity_id)?
        .energy
        .fuel_slot_mut()
        .expect("a planned drill fuel transfer targets a burner drill");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(fuel_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn mining_drill_output_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, MiningDrillError> {
    let output_slot = sim.entities.mining_drill_state(entity_id)?.output_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&output_slot),
            slot_index: MINING_DRILL_OUTPUT_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::OutputOnly,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, MiningDrillError::InvalidFuel))?;

    let output_slot = &mut sim.entities.mining_drill_state_mut(entity_id)?.output_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(output_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn player_slot_to_inserter_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, InserterError> {
    let fuel_slot = sim
        .entities
        .inserter_energy(entity_id)?
        .fuel_slot()
        .ok_or(InserterError::NoFuelSlot)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(&fuel_slot),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, InserterError::InvalidFuel))?;

    let fuel_slot = sim
        .entities
        .inserter_energy_mut(entity_id)?
        .fuel_slot_mut()
        .expect("a planned inserter fuel transfer targets a burner inserter");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
        TransferDestinationMut::SingleSlot(fuel_slot),
    ))
}

pub fn inserter_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, InserterError> {
    let fuel_slot = sim
        .entities
        .inserter_energy(entity_id)?
        .fuel_slot()
        .ok_or(InserterError::NoFuelSlot)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&fuel_slot),
            slot_index: INSERTER_FUEL_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, InserterError::InvalidFuel))?;

    let fuel_slot = sim
        .entities
        .inserter_energy_mut(entity_id)?
        .fuel_slot_mut()
        .expect("a planned inserter fuel transfer targets a burner inserter");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(fuel_slot),
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
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(&input_slot),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::FurnaceIngredient,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, FurnaceError::InvalidInput))?;

    let outcome = {
        let input_slot = &mut sim.entities.furnace_state_mut(entity_id)?.input_slot;
        commit_transfer(
            plan,
            TransferSourceMut::Slot(
                sim.player_inventory
                    .item_slot_mut(player_slot_index)
                    .expect("a planned player source slot remains in bounds"),
            ),
            TransferDestinationMut::SingleSlot(input_slot),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
}

pub fn player_slot_to_furnace_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, FurnaceError> {
    let fuel_slot = sim
        .entities
        .furnace_state(entity_id)?
        .energy
        .fuel_slot()
        .ok_or(FurnaceError::NoFuelSlot)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(&fuel_slot),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, FurnaceError::InvalidFuel))?;

    let fuel_slot = sim
        .entities
        .furnace_state_mut(entity_id)?
        .energy
        .fuel_slot_mut()
        .expect("a planned furnace fuel transfer targets a burner furnace");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
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
        ItemSlotPolicy::FurnaceIngredient,
        |state| &mut state.input_slot,
    )
}

pub fn furnace_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, FurnaceError> {
    let fuel_slot = sim
        .entities
        .furnace_state(entity_id)?
        .energy
        .fuel_slot()
        .ok_or(FurnaceError::NoFuelSlot)?;
    transfer_furnace_slot_to_player(
        sim,
        entity_id,
        fuel_slot,
        FURNACE_FUEL_SLOT_INDEX,
        ItemSlotPolicy::Fuel,
        |state| {
            state
                .energy
                .fuel_slot_mut()
                .expect("a planned furnace fuel transfer targets a burner furnace")
        },
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
        ItemSlotPolicy::OutputOnly,
        |state| &mut state.output_slot,
    )
}

fn transfer_furnace_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot: ItemSlot,
    slot_index: usize,
    policy: ItemSlotPolicy,
    slot_mut: impl FnOnce(&mut FurnaceState) -> &mut ItemSlot,
) -> Result<TransferOutcome, FurnaceError> {
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&slot),
            slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                policy,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, FurnaceError::InvalidInput))?;

    let outcome = {
        let source = slot_mut(sim.entities.furnace_state_mut(entity_id)?);
        commit_transfer(
            plan,
            TransferSourceMut::Slot(source),
            TransferDestinationMut::Inventory(&mut sim.player_inventory),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
}

pub fn player_slot_to_boiler_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, BoilerError> {
    let fuel_slot = sim.entities.boiler_state(entity_id)?.energy.fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::SingleSlot(&fuel_slot),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, BoilerError::InvalidFuel))?;

    let fuel_slot = &mut sim.entities.boiler_state_mut(entity_id)?.energy.fuel_slot;
    let outcome = commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
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
        TransferSource {
            slot: Some(&fuel_slot),
            slot_index: BOILER_FUEL_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, BoilerError::InvalidFuel))?;

    let fuel_slot = &mut sim.entities.boiler_state_mut(entity_id)?.energy.fuel_slot;
    let outcome = commit_transfer(
        plan,
        TransferSourceMut::Slot(fuel_slot),
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
    let state = sim.entities.assembler_state(entity_id)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(&state.input_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::AssemblerIngredient(entity_id),
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, AssemblerError::InvalidInput))?;

    let outcome = {
        let input_inventory = &mut sim.entities.assembler_state_mut(entity_id)?.input_inventory;
        commit_transfer(
            plan,
            TransferSourceMut::Slot(
                sim.player_inventory
                    .item_slot_mut(player_slot_index)
                    .expect("a planned player source slot remains in bounds"),
            ),
            TransferDestinationMut::Inventory(input_inventory),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
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
        ItemSlotPolicy::AssemblerIngredient(entity_id),
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
        ItemSlotPolicy::OutputOnly,
        |state| &state.output_inventory,
        |state| &mut state.output_inventory,
    )
}

fn transfer_assembler_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
    policy: ItemSlotPolicy,
    inventory: impl FnOnce(&AssemblingMachineState) -> &Inventory,
    inventory_mut: impl FnOnce(&mut AssemblingMachineState) -> &mut Inventory,
) -> Result<TransferOutcome, AssemblerError> {
    let source_inventory = inventory(sim.entities.assembler_state(entity_id)?);
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: source_inventory.item_slot(slot_index),
            slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                policy,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, AssemblerError::InvalidInput))?;

    let outcome = {
        let source_inventory = inventory_mut(sim.entities.assembler_state_mut(entity_id)?);
        commit_transfer(
            plan,
            TransferSourceMut::Slot(
                source_inventory
                    .item_slot_mut(slot_index)
                    .expect("a planned assembler source slot remains in bounds"),
            ),
            TransferDestinationMut::Inventory(&mut sim.player_inventory),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
}

fn player_slot_to_furnace(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, FurnaceError> {
    let stack = sim
        .player_inventory()
        .item_slot(slot_index)
        .ok_or(FurnaceError::InvalidSlot { slot_index })?
        .stack()
        .ok_or(FurnaceError::EmptySlot { slot_index })?;
    let has_fuel_slot = sim
        .entities
        .furnace_state(entity_id)?
        .energy
        .fuel_slot()
        .is_some();
    let is_fuel = has_fuel_slot
        && item_slot_policy_accepts(
            sim.catalog(),
            &sim.research,
            &sim.entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::PlayerInsert,
            stack.item_id(),
        );

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
                test_slot(ItemStack::new(&catalog, item_id, 40).unwrap()),
                test_slot(ItemStack::new(&catalog, item_id, 60).unwrap()),
            ],
        )
        .unwrap();
        let mut destination = Inventory::from_slots(
            &catalog,
            vec![test_slot(ItemStack::new(&catalog, item_id, 75).unwrap())],
        )
        .unwrap();

        let plan = plan_transfer(
            &catalog,
            TransferSource {
                slot: source.item_slot(0),
                slot_index: 0,
            },
            TransferDestination::Inventory(&destination),
            |_| true,
        )
        .unwrap();
        let outcome = commit_transfer(
            plan,
            TransferSourceMut::Slot(source.item_slot_mut(0).unwrap()),
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
            vec![test_slot(ItemStack::new(&catalog, iron_plate, 10).unwrap())],
        )
        .unwrap();
        let destination = Inventory::from_slots(
            &catalog,
            vec![test_slot(ItemStack::new(&catalog, coal, 100).unwrap())],
        )
        .unwrap();
        let source_before = source.clone();
        let destination_before = destination.clone();

        assert_eq!(
            plan_transfer(
                &catalog,
                TransferSource {
                    slot: source.item_slot(0),
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
            vec![test_slot(
                ItemStack::new(&source_catalog, item_id, 1).unwrap(),
            )],
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
                TransferSource {
                    slot: source.item_slot(0),
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
