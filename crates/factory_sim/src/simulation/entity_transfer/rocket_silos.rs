use super::*;

/// Moves one player slot into a silo's ingredient inventory.
///
/// There is no counterpart for products: a silo has no output inventory, so the
/// only two directions a player can move items are into the ingredient slots and
/// back out of them again.
pub fn player_slot_to_rocket_silo_input(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, RocketSiloError> {
    let state = sim.entities.rocket_silo_state(entity_id)?;
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
                ItemSlotPolicy::RocketPartIngredient,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, RocketSiloError::InvalidInput))?;

    let outcome = {
        let input_inventory = &mut sim
            .entities
            .rocket_silo_state_mut(entity_id)?
            .input_inventory;
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

pub fn rocket_silo_input_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, RocketSiloError> {
    let source_inventory = &sim.entities.rocket_silo_state(entity_id)?.input_inventory;
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
                ItemSlotPolicy::RocketPartIngredient,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, RocketSiloError::InvalidInput))?;

    let outcome = {
        let source_inventory = &mut sim
            .entities
            .rocket_silo_state_mut(entity_id)?
            .input_inventory;
        commit_transfer(
            plan,
            TransferSourceMut::Slot(
                source_inventory
                    .item_slot_mut(slot_index)
                    .expect("a planned rocket silo source slot remains in bounds"),
            ),
            TransferDestinationMut::Inventory(&mut sim.player_inventory),
        )
    };
    sim.invalidate_consumer_power_demand(entity_id);
    Ok(outcome)
}
