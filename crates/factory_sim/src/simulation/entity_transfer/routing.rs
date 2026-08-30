use super::*;
pub fn transfer_container_slot(
    sim: &mut Simulation,
    entity_id: EntityId,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<TransferOutcome, SlotTransferError> {
    match panel {
        InventoryPanel::Player => {
            if let Some(stack) = sim.player_inventory.slot(slot_index) {
                let is_module = sim
                    .world
                    .prototypes
                    .item(stack.item_id())
                    .is_some_and(|item| item.module_effect.is_some());
                let is_beacon =
                    entity_access::machine_kind(sim, entity_id) == Some(EntityKind::Beacon);
                let has_module_state = entity_access::module_slots(sim, entity_id).is_ok();
                if is_beacon || (is_module && has_module_state) {
                    return player_slot_to_modules(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Module);
                }
            }
            match entity_access::machine_kind(sim, entity_id) {
                Some(EntityKind::MiningDrill) => {
                    return player_slot_to_mining_drill_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::MiningDrill);
                }
                Some(EntityKind::Furnace) => {
                    return player_slot_to_furnace(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Furnace);
                }
                Some(EntityKind::Boiler) => {
                    return player_slot_to_boiler_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Boiler);
                }
                Some(EntityKind::NuclearReactor) => {
                    return player_slot_to_nuclear_reactor_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::NuclearReactor);
                }
                Some(EntityKind::Roboport) => {
                    return player_slot_to_roboport(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Roboport);
                }
                Some(EntityKind::Inserter) => {
                    return player_slot_to_inserter_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Inserter);
                }
                Some(EntityKind::AssemblingMachine) => {
                    return player_slot_to_assembler_input(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Assembler);
                }
                Some(EntityKind::RocketSilo) => {
                    let is_launchable_cargo = sim
                        .entities
                        .rocket_silo_state(entity_id)
                        .is_ok_and(|state| state.rocket_ready())
                        && sim.player_inventory.slot(slot_index).is_some_and(|stack| {
                            item_slot_policy_accepts(
                                sim.catalog(),
                                &sim.research,
                                &sim.entities,
                                ItemSlotPolicy::RocketCargo,
                                ItemSlotOperation::PlayerInsert,
                                stack.item_id(),
                            )
                        });
                    if is_launchable_cargo {
                        return player_slot_to_rocket_silo_cargo(sim, entity_id, slot_index)
                            .map_err(SlotTransferError::RocketSilo);
                    }
                    return player_slot_to_rocket_silo_input(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::RocketSilo);
                }
                _ => {}
            }
            player_slot_to_entity(sim, entity_id, slot_index)
        }
        InventoryPanel::Container => entity_slot_to_player(sim, entity_id, slot_index),
        InventoryPanel::BurnerFuel => {
            return mining_drill_fuel_to_player(sim, entity_id)
                .map_err(SlotTransferError::MiningDrill);
        }
        InventoryPanel::BurnerOutput => {
            return mining_drill_output_to_player(sim, entity_id)
                .map_err(SlotTransferError::MiningDrill);
        }
        InventoryPanel::FurnaceInput => {
            return furnace_input_to_player(sim, entity_id).map_err(SlotTransferError::Furnace);
        }
        InventoryPanel::FurnaceFuel => {
            return furnace_fuel_to_player(sim, entity_id).map_err(SlotTransferError::Furnace);
        }
        InventoryPanel::FurnaceOutput => {
            return furnace_output_to_player(sim, entity_id).map_err(SlotTransferError::Furnace);
        }
        InventoryPanel::BoilerFuel => {
            return boiler_fuel_to_player(sim, entity_id).map_err(SlotTransferError::Boiler);
        }
        InventoryPanel::NuclearReactorFuel => {
            return nuclear_reactor_fuel_to_player(sim, entity_id)
                .map_err(SlotTransferError::NuclearReactor);
        }
        InventoryPanel::NuclearReactorOutput => {
            return nuclear_reactor_output_to_player(sim, entity_id)
                .map_err(SlotTransferError::NuclearReactor);
        }
        InventoryPanel::RoboportRobots => {
            return roboport_slot_to_player(sim, entity_id, slot_index, RoboportInventory::Robots)
                .map_err(SlotTransferError::Roboport);
        }
        InventoryPanel::RoboportMaterial => {
            return roboport_slot_to_player(
                sim,
                entity_id,
                slot_index,
                RoboportInventory::Materials,
            )
            .map_err(SlotTransferError::Roboport);
        }
        InventoryPanel::InserterFuel => {
            return inserter_fuel_to_player(sim, entity_id).map_err(SlotTransferError::Inserter);
        }
        InventoryPanel::AssemblerInput => {
            return assembler_input_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Assembler);
        }
        InventoryPanel::AssemblerOutput => {
            return assembler_output_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Assembler);
        }
        InventoryPanel::RocketSiloInput => {
            return rocket_silo_input_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::RocketSilo);
        }
        InventoryPanel::RocketSiloCargo => {
            return rocket_silo_cargo_to_player(sim, entity_id)
                .map_err(SlotTransferError::RocketSilo);
        }
        InventoryPanel::RocketSiloOutput => {
            return rocket_silo_output_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::RocketSilo);
        }
        InventoryPanel::Modules => {
            return module_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Module);
        }
        // A wagon is not an entity, so a click in its window never arrives
        // here; it goes to `transfer_rolling_stock_slot` instead.
        InventoryPanel::RollingStockCargo | InventoryPanel::RollingStockFuel => {
            return Err(SlotTransferError::RollingStock(
                RollingStockTransferError::UnsupportedPanel,
            ));
        }
    }
    .map_err(SlotTransferError::Transfer)
}

fn player_slot_to_furnace(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, FurnaceError> {
    let stack = sim
        .player_inventory()
        .item_slot(slot_index)
        .ok_or(FurnaceError::InvalidSlot { slot_index })?
        .stack()
        .ok_or(FurnaceError::EmptySlot { slot_index })?;
    let has_fuel_slot = sim
        .entities
        .furnace_state(entity_id)?
        .energy
        .fuel_slot()
        .is_some();
    let is_fuel = has_fuel_slot
        && item_slot_policy_accepts(
            sim.catalog(),
            &sim.research,
            &sim.entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::PlayerInsert,
            stack.item_id(),
        );

    if is_fuel {
        player_slot_to_furnace_fuel(sim, entity_id, slot_index)
    } else {
        player_slot_to_furnace_input(sim, entity_id, slot_index)
    }
}

/// Routes one click in a rolling-stock window to the transfer it means.
///
/// The stock counterpart of [`transfer_container_slot`], and split from it for
/// the same reason the transfers themselves are: the endpoint is a
/// [`RollingStockId`] rather than an [`EntityId`], and a router that took
/// either would be a router that has to guess which it was given.
pub fn transfer_rolling_stock_slot(
    sim: &mut Simulation,
    stock_id: RollingStockId,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<TransferOutcome, RollingStockTransferError> {
    match panel {
        // Which half of a piece a player's item goes to follows from the piece:
        // a locomotive has only a fuel slot and a cargo wagon only an
        // inventory, so there is nothing to disambiguate and no need to ask
        // whether the item burns.
        InventoryPanel::Player => {
            if sim
                .rolling_stock
                .get(stock_id)
                .is_some_and(|stock| stock.inventory.is_some())
            {
                player_slot_to_rolling_stock(sim, stock_id, slot_index)
            } else {
                player_slot_to_rolling_stock_fuel(sim, stock_id, slot_index)
            }
        }
        InventoryPanel::RollingStockCargo => {
            rolling_stock_slot_to_player(sim, stock_id, slot_index)
        }
        InventoryPanel::RollingStockFuel => rolling_stock_fuel_to_player(sim, stock_id),
        _ => Err(RollingStockTransferError::UnsupportedPanel),
    }
}
