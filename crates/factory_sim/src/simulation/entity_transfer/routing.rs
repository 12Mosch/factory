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
        InventoryPanel::Modules => {
            return module_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Module);
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
