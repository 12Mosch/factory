use super::*;
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
