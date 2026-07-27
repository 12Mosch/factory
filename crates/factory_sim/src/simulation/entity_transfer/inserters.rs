use super::*;
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
