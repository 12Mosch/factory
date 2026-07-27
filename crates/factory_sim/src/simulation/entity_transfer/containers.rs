use super::*;
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
