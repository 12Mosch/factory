use super::*;
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

pub fn player_slot_to_nuclear_reactor_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, NuclearReactorError> {
    let fuel_slot = sim
        .entities
        .nuclear_reactor_state(entity_id)?
        .energy
        .fuel_slot;
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
    .map_err(|error| map_plan_error(error, NuclearReactorError::InvalidFuel))?;

    let fuel_slot = &mut sim
        .entities
        .nuclear_reactor_state_mut(entity_id)?
        .energy
        .fuel_slot;
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

pub fn nuclear_reactor_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, NuclearReactorError> {
    let fuel_slot = sim
        .entities
        .nuclear_reactor_state(entity_id)?
        .energy
        .fuel_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&fuel_slot),
            slot_index: NUCLEAR_REACTOR_FUEL_SLOT_INDEX,
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
    .map_err(|error| map_plan_error(error, NuclearReactorError::InvalidFuel))?;

    let fuel_slot = &mut sim
        .entities
        .nuclear_reactor_state_mut(entity_id)?
        .energy
        .fuel_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(fuel_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

pub fn nuclear_reactor_output_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, NuclearReactorError> {
    let output_slot = sim.entities.nuclear_reactor_state(entity_id)?.output_slot;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&output_slot),
            slot_index: NUCLEAR_REACTOR_OUTPUT_SLOT_INDEX,
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
    .map_err(|error| map_plan_error(error, NuclearReactorError::InvalidOutput))?;

    let output_slot = &mut sim
        .entities
        .nuclear_reactor_state_mut(entity_id)?
        .output_slot;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(output_slot),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}
