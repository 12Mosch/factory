use super::*;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match *command {
        SimCommand::StartManualCraft(recipe_id) => sim
            .start_manual_craft(recipe_id)
            .map_err(SimCommandError::Crafting)?,
        SimCommand::CancelManualCraft { job_id } => sim
            .cancel_manual_craft(job_id)
            .map_err(SimCommandError::Crafting)?,
        SimCommand::MoveManualCraft { job_id, direction } => sim
            .move_manual_craft(job_id, direction)
            .map_err(SimCommandError::Crafting)?,
        SimCommand::SelectAssemblerRecipe {
            entity_id,
            recipe_id,
        } => sim
            .select_assembler_recipe(entity_id, recipe_id)
            .map_err(SimCommandError::Assembler)?,
        SimCommand::EnqueueResearch(technology_id) => sim
            .enqueue_research(technology_id)
            .map_err(SimCommandError::Research)?,
        SimCommand::RemoveQueuedResearch { index } => {
            sim.remove_queued_research(index)
                .map_err(SimCommandError::Research)?;
        }
        SimCommand::MoveQueuedResearch {
            from_index,
            to_index,
        } => sim
            .move_queued_research(from_index, to_index)
            .map_err(SimCommandError::Research)?,
        SimCommand::TransferSlot {
            entity_id,
            panel,
            slot_index,
        } => {
            entity_transfer::transfer_container_slot(sim, entity_id, panel, slot_index)
                .map_err(SimCommandError::Transfer)?;
        }
        SimCommand::TransferRollingStockSlot {
            stock_id,
            panel,
            slot_index,
        } => {
            entity_transfer::transfer_rolling_stock_slot(sim, stock_id, panel, slot_index)
                .map_err(|error| {
                    SimCommandError::Transfer(SlotTransferError::RollingStock(error))
                })?;
        }
        SimCommand::SetRollingStockSlotFilter {
            stock_id,
            slot_index,
            filter,
        } => entity_transfer::set_rolling_stock_slot_filter(sim, stock_id, slot_index, filter)
            .map_err(|error| SimCommandError::Transfer(SlotTransferError::RollingStock(error)))?,
        SimCommand::SetLogisticRequest {
            entity_id,
            slot_index,
            request,
        } => sim
            .set_logistic_request(entity_id, slot_index, request)
            .map_err(SimCommandError::LogisticChest)?,
        _ => unreachable!("non-production command routed to production dispatcher"),
    }
    Ok(SimCommandEffect::None)
}
