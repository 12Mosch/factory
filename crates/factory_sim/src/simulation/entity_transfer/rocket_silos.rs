use super::*;
use crate::machines::RocketLaunchPhase;

/// Moves one player slot into a silo's ingredient inventory.
///
/// Launch products have their own output-only route below; they never enter the
/// ingredient inventory through this path.
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

/// Moves one launchable payload into the completed rocket's cargo slot.
pub fn player_slot_to_rocket_silo_cargo(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, RocketSiloError> {
    let state = sim.entities.rocket_silo_state(entity_id)?;
    if !state.rocket_ready() || !matches!(state.launch_phase, RocketLaunchPhase::Idle) {
        return Err(RocketSiloError::InsufficientSpace);
    }
    if !state
        .cargo_inventory
        .slots()
        .first()
        .is_some_and(|slot| slot.is_empty())
    {
        return Err(RocketSiloError::InsufficientSpace);
    }
    let plan = plan_transfer_limited(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(&state.cargo_inventory),
        1,
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::RocketCargo,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, RocketSiloError::InvalidInput))?;

    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
        TransferDestinationMut::Inventory(
            &mut sim
                .entities
                .rocket_silo_state_mut(entity_id)?
                .cargo_inventory,
        ),
    ))
}

/// Removes the waiting payload before launch begins.
pub fn rocket_silo_cargo_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<TransferOutcome, RocketSiloError> {
    let state = sim.entities.rocket_silo_state(entity_id)?;
    if !matches!(state.launch_phase, RocketLaunchPhase::Idle) {
        return Err(RocketSiloError::InsufficientSpace);
    }
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: state.cargo_inventory.item_slot(0),
            slot_index: 0,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, RocketSiloError::InvalidInput))?;
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.entities
                .rocket_silo_state_mut(entity_id)?
                .cargo_inventory
                .item_slot_mut(0)
                .expect("cargo has exactly one slot"),
        ),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}

/// Moves one launch-product slot into the player's inventory.
pub fn rocket_silo_output_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, RocketSiloError> {
    let output = &sim.entities.rocket_silo_state(entity_id)?.output_inventory;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: output.item_slot(slot_index),
            slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, RocketSiloError::InvalidInput))?;

    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            sim.entities
                .rocket_silo_state_mut(entity_id)?
                .output_inventory
                .item_slot_mut(slot_index)
                .expect("a planned rocket silo output slot remains in bounds"),
        ),
        TransferDestinationMut::Inventory(&mut sim.player_inventory),
    ))
}
