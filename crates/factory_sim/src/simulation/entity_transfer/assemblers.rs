use super::*;
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
