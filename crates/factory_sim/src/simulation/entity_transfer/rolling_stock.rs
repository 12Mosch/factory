//! Player transfers into and out of rolling stock.
//!
//! The entity path next door resolves its endpoint by [`EntityId`]; rolling
//! stock has none, so the endpoint is a [`RollingStockId`] instead. Everything
//! else is the shared planner: validation completes before either side is
//! touched, so a refused transfer leaves the player's inventory and the wagon
//! exactly as they were.
//!
//! Reaching a wagon is not gated on the train being stopped, and that is
//! deliberate. The stopped rule exists because an inserter is a fixed machine
//! that would otherwise be reaching at a moving target; a player who has a
//! wagon's window open is standing beside it, and closing the window when a
//! train creeps forward would be a rule about the interface rather than about
//! the world.

use super::*;
use crate::rolling_stock::{RollingStockId, RollingStockTransferError};

/// The wagon's own inventory, or an error naming what it has instead.
fn stock_inventory(
    sim: &Simulation,
    stock_id: RollingStockId,
) -> Result<&Inventory, RollingStockTransferError> {
    sim.rolling_stock
        .get(stock_id)
        .ok_or(RollingStockTransferError::MissingStock(stock_id))?
        .inventory
        .as_ref()
        .ok_or(RollingStockTransferError::NoInventory(stock_id))
}

fn stock_inventory_mut(
    sim: &mut Simulation,
    stock_id: RollingStockId,
) -> Result<&mut Inventory, RollingStockTransferError> {
    sim.rolling_stock
        .get_mut(stock_id)
        .ok_or(RollingStockTransferError::MissingStock(stock_id))?
        .inventory
        .as_mut()
        .ok_or(RollingStockTransferError::NoInventory(stock_id))
}

pub fn player_slot_to_rolling_stock(
    sim: &mut Simulation,
    stock_id: RollingStockId,
    player_slot_index: usize,
) -> Result<TransferOutcome, RollingStockTransferError> {
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: sim.player_inventory.item_slot(player_slot_index),
            slot_index: player_slot_index,
        },
        TransferDestination::Inventory(stock_inventory(sim, stock_id)?),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Unrestricted,
                ItemSlotOperation::PlayerInsert,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, RollingStockTransferError::InvalidItem))?;

    let Simulation {
        player_inventory,
        rolling_stock,
        ..
    } = sim;
    let inventory = rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.inventory.as_mut())
        .expect("the wagon inventory was just read");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
        TransferDestinationMut::Inventory(inventory),
    ))
}

pub fn rolling_stock_slot_to_player(
    sim: &mut Simulation,
    stock_id: RollingStockId,
    stock_slot_index: usize,
) -> Result<TransferOutcome, RollingStockTransferError> {
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: stock_inventory(sim, stock_id)?.item_slot(stock_slot_index),
            slot_index: stock_slot_index,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |item_id| {
            item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Unrestricted,
                ItemSlotOperation::PlayerExtract,
                item_id,
            )
        },
    )
    .map_err(|error| map_plan_error(error, RollingStockTransferError::InvalidItem))?;

    let Simulation {
        player_inventory,
        rolling_stock,
        ..
    } = sim;
    let source = rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.inventory.as_mut())
        .expect("the wagon inventory was just read")
        .item_slot_mut(stock_slot_index)
        .expect("a planned wagon source slot remains in bounds");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(source),
        TransferDestinationMut::Inventory(player_inventory),
    ))
}

pub fn player_slot_to_rolling_stock_fuel(
    sim: &mut Simulation,
    stock_id: RollingStockId,
    player_slot_index: usize,
) -> Result<TransferOutcome, RollingStockTransferError> {
    let fuel_slot = rolling_stock_fuel_slot(sim, stock_id)?;
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
    .map_err(|error| map_plan_error(error, RollingStockTransferError::InvalidItem))?;

    let Simulation {
        player_inventory,
        rolling_stock,
        ..
    } = sim;
    let destination = rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.energy.as_mut())
        .map(|energy| &mut energy.fuel_slot)
        .expect("the locomotive fuel slot was just read");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(
            player_inventory
                .item_slot_mut(player_slot_index)
                .expect("a planned player source slot remains in bounds"),
        ),
        TransferDestinationMut::SingleSlot(destination),
    ))
}

pub fn rolling_stock_fuel_to_player(
    sim: &mut Simulation,
    stock_id: RollingStockId,
) -> Result<TransferOutcome, RollingStockTransferError> {
    let fuel_slot = rolling_stock_fuel_slot(sim, stock_id)?;
    let plan = plan_transfer(
        &sim.world.prototypes,
        TransferSource {
            slot: Some(&fuel_slot),
            slot_index: crate::simulation::ROLLING_STOCK_FUEL_SLOT_INDEX,
        },
        TransferDestination::Inventory(&sim.player_inventory),
        |_| true,
    )
    .map_err(|error| map_plan_error(error, RollingStockTransferError::InvalidItem))?;

    let Simulation {
        player_inventory,
        rolling_stock,
        ..
    } = sim;
    let source = rolling_stock
        .get_mut(stock_id)
        .and_then(|stock| stock.energy.as_mut())
        .map(|energy| &mut energy.fuel_slot)
        .expect("the locomotive fuel slot was just read");
    Ok(commit_transfer(
        plan,
        TransferSourceMut::Slot(source),
        TransferDestinationMut::Inventory(player_inventory),
    ))
}

/// Filters one cargo slot of a wagon, or clears its filter.
///
/// A player action rather than a transfer, but it belongs beside them: what a
/// filter does is decide which of these transfers a slot will take.
pub fn set_rolling_stock_slot_filter(
    sim: &mut Simulation,
    stock_id: RollingStockId,
    slot_index: usize,
    filter: Option<ItemId>,
) -> Result<(), RollingStockTransferError> {
    if let Some(item_id) = filter
        && sim.world.prototypes.item(item_id).is_none()
    {
        return Err(RollingStockTransferError::InvalidItem(item_id));
    }
    stock_inventory_mut(sim, stock_id)?
        .set_filter(slot_index, filter)
        .map_err(|error| match error {
            InventoryError::FilterMismatch { slot_index } => {
                RollingStockTransferError::SlotNotEmpty { slot_index }
            }
            other => RollingStockTransferError::from(other),
        })
}

fn rolling_stock_fuel_slot(
    sim: &Simulation,
    stock_id: RollingStockId,
) -> Result<ItemSlot, RollingStockTransferError> {
    sim.rolling_stock
        .get(stock_id)
        .ok_or(RollingStockTransferError::MissingStock(stock_id))?
        .energy
        .as_ref()
        .map(|energy| energy.fuel_slot)
        .ok_or(RollingStockTransferError::NoFuelSlot(stock_id))
}
