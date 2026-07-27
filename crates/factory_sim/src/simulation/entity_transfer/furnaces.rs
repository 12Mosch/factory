use super::*;
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
