use super::*;

pub fn transfer_container_slot(
    sim: &mut Simulation,
    entity_id: EntityId,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<(), SlotTransferError> {
    match panel {
        InventoryPanel::Player => {
            match entity_access::machine_kind(sim, entity_id) {
                Some(EntityKind::MiningDrill) => {
                    return player_slot_to_burner_drill_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::BurnerDrill);
                }
                Some(EntityKind::Furnace) => {
                    return player_slot_to_furnace(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Furnace);
                }
                Some(EntityKind::Boiler) => {
                    return player_slot_to_boiler_fuel(sim, entity_id, slot_index)
                        .map_err(SlotTransferError::Boiler);
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
            return burner_drill_fuel_to_player(sim, entity_id)
                .map_err(SlotTransferError::BurnerDrill);
        }
        InventoryPanel::BurnerOutput => {
            return burner_drill_output_to_player(sim, entity_id)
                .map_err(SlotTransferError::BurnerDrill);
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
        InventoryPanel::AssemblerInput => {
            return assembler_input_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Assembler);
        }
        InventoryPanel::AssemblerOutput => {
            return assembler_output_slot_to_player(sim, entity_id, slot_index)
                .map_err(SlotTransferError::Assembler);
        }
    }
    .map_err(SlotTransferError::Transfer)
}

pub fn player_slot_to_entity(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<(), ContainerError> {
    let stack = stack_in_slot(&sim.player_inventory, player_slot_index)?;
    if sim.entities.labs.contains_key(&entity_id)
        && !lab_can_accept_item(&sim.world.prototypes, stack.item_id)
    {
        return Err(ContainerError::InvalidItem(stack.item_id));
    }
    let entity_inventory = EntityStore::entity_inventory(&sim.entities, entity_id)?;
    ensure_inventory_can_accept(&sim.world.prototypes, entity_inventory, stack)?;

    sim.player_inventory.slots[player_slot_index] = None;
    EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)?
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(ContainerError::from)
}

pub fn entity_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    entity_slot_index: usize,
) -> Result<(), ContainerError> {
    let stack = {
        let entity_inventory = EntityStore::entity_inventory(&sim.entities, entity_id)?;
        stack_in_slot(entity_inventory, entity_slot_index)?
    };
    ensure_inventory_can_accept(&sim.world.prototypes, &sim.player_inventory, stack)?;

    EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)?.slots[entity_slot_index] =
        None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(ContainerError::from)
}

pub fn player_slot_to_burner_drill_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<(), BurnerDrillError> {
    let stack = sim
        .player_inventory
        .slots
        .get(player_slot_index)
        .ok_or(BurnerDrillError::InvalidSlot {
            slot_index: player_slot_index,
        })?
        .ok_or(BurnerDrillError::EmptySlot {
            slot_index: player_slot_index,
        })?;

    if fuel_value_joules(&sim.world.prototypes, stack.item_id).is_none() {
        return Err(BurnerDrillError::InvalidFuel(stack.item_id));
    }

    let state = sim.entities.burner_drill_state(entity_id)?;
    if !burner_fuel_slot_can_accept(&sim.world.prototypes, state.energy.fuel_slot, stack) {
        return Err(BurnerDrillError::InsufficientSpace);
    }

    sim.player_inventory.slots[player_slot_index] = None;
    let state = sim.entities.burner_drill_state_mut(entity_id)?;
    insert_into_single_slot(&mut state.energy.fuel_slot, stack);

    Ok(())
}

pub fn burner_drill_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<(), BurnerDrillError> {
    let stack = sim
        .entities
        .burner_drill_state(entity_id)?
        .energy
        .fuel_slot
        .ok_or(BurnerDrillError::EmptySlot {
            slot_index: BURNER_MINING_DRILL_FUEL_SLOT_INDEX,
        })?;
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(BurnerDrillError::InsufficientSpace);
    }

    sim.entities
        .burner_drill_state_mut(entity_id)?
        .energy
        .fuel_slot = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(BurnerDrillError::from)
}

pub fn burner_drill_output_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<(), BurnerDrillError> {
    let stack = sim
        .entities
        .burner_drill_state(entity_id)?
        .output_slot
        .ok_or(BurnerDrillError::EmptySlot {
            slot_index: BURNER_MINING_DRILL_OUTPUT_SLOT_INDEX,
        })?;
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(BurnerDrillError::InsufficientSpace);
    }

    sim.entities.burner_drill_state_mut(entity_id)?.output_slot = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(BurnerDrillError::from)
}

pub fn player_slot_to_furnace_input(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<(), FurnaceError> {
    let stack = sim
        .player_inventory
        .slots
        .get(player_slot_index)
        .ok_or(FurnaceError::InvalidSlot {
            slot_index: player_slot_index,
        })?
        .ok_or(FurnaceError::EmptySlot {
            slot_index: player_slot_index,
        })?;

    if first_matching_unlocked_smelting_recipe(&sim.world.prototypes, &sim.research, stack.item_id)
        .is_none()
    {
        return Err(FurnaceError::InvalidInput(stack.item_id));
    }

    let state = sim.entities.furnace_state(entity_id)?;
    if !input_slot_can_accept(
        &sim.world.prototypes,
        &sim.research,
        state.input_slot,
        stack,
    ) {
        return Err(FurnaceError::InsufficientSpace);
    }

    sim.player_inventory.slots[player_slot_index] = None;
    let state = sim.entities.furnace_state_mut(entity_id)?;
    insert_into_single_slot(&mut state.input_slot, stack);

    Ok(())
}

pub fn player_slot_to_furnace_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<(), FurnaceError> {
    let stack = sim
        .player_inventory
        .slots
        .get(player_slot_index)
        .ok_or(FurnaceError::InvalidSlot {
            slot_index: player_slot_index,
        })?
        .ok_or(FurnaceError::EmptySlot {
            slot_index: player_slot_index,
        })?;

    if fuel_value_joules(&sim.world.prototypes, stack.item_id).is_none() {
        return Err(FurnaceError::InvalidFuel(stack.item_id));
    }

    let state = sim.entities.furnace_state(entity_id)?;
    if !burner_fuel_slot_can_accept(&sim.world.prototypes, state.energy.fuel_slot, stack) {
        return Err(FurnaceError::InsufficientSpace);
    }

    sim.player_inventory.slots[player_slot_index] = None;
    let state = sim.entities.furnace_state_mut(entity_id)?;
    insert_into_single_slot(&mut state.energy.fuel_slot, stack);

    Ok(())
}

pub fn furnace_input_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<(), FurnaceError> {
    let stack =
        sim.entities
            .furnace_state(entity_id)?
            .input_slot
            .ok_or(FurnaceError::EmptySlot {
                slot_index: FURNACE_INPUT_SLOT_INDEX,
            })?;
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(FurnaceError::InsufficientSpace);
    }

    sim.entities.furnace_state_mut(entity_id)?.input_slot = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(FurnaceError::from)
}

pub fn furnace_fuel_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<(), FurnaceError> {
    let stack = sim
        .entities
        .furnace_state(entity_id)?
        .energy
        .fuel_slot
        .ok_or(FurnaceError::EmptySlot {
            slot_index: FURNACE_FUEL_SLOT_INDEX,
        })?;
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(FurnaceError::InsufficientSpace);
    }

    sim.entities.furnace_state_mut(entity_id)?.energy.fuel_slot = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(FurnaceError::from)
}

pub fn furnace_output_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<(), FurnaceError> {
    let stack =
        sim.entities
            .furnace_state(entity_id)?
            .output_slot
            .ok_or(FurnaceError::EmptySlot {
                slot_index: FURNACE_OUTPUT_SLOT_INDEX,
            })?;
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(FurnaceError::InsufficientSpace);
    }

    sim.entities.furnace_state_mut(entity_id)?.output_slot = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(FurnaceError::from)
}

pub fn player_slot_to_boiler_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<(), BoilerError> {
    let stack = sim
        .player_inventory
        .slots
        .get(player_slot_index)
        .ok_or(BoilerError::InvalidSlot {
            slot_index: player_slot_index,
        })?
        .ok_or(BoilerError::EmptySlot {
            slot_index: player_slot_index,
        })?;

    if fuel_value_joules(&sim.world.prototypes, stack.item_id).is_none() {
        return Err(BoilerError::InvalidFuel(stack.item_id));
    }

    let state = sim.entities.boiler_state(entity_id)?;
    if !burner_fuel_slot_can_accept(&sim.world.prototypes, state.energy.fuel_slot, stack) {
        return Err(BoilerError::InsufficientSpace);
    }

    sim.player_inventory.slots[player_slot_index] = None;
    let state = sim.entities.boiler_state_mut(entity_id)?;
    insert_into_single_slot(&mut state.energy.fuel_slot, stack);
    sim.invalidate_power_dynamic_state();

    Ok(())
}

pub fn boiler_fuel_to_player(sim: &mut Simulation, entity_id: EntityId) -> Result<(), BoilerError> {
    let stack = sim
        .entities
        .boiler_state(entity_id)?
        .energy
        .fuel_slot
        .ok_or(BoilerError::EmptySlot {
            slot_index: BOILER_FUEL_SLOT_INDEX,
        })?;
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(BoilerError::InsufficientSpace);
    }

    sim.entities.boiler_state_mut(entity_id)?.energy.fuel_slot = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(BoilerError::from)?;
    sim.invalidate_power_dynamic_state();
    Ok(())
}

pub fn player_slot_to_assembler_input(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<(), AssemblerError> {
    let stack = sim
        .player_inventory
        .slots
        .get(player_slot_index)
        .ok_or(AssemblerError::InvalidSlot {
            slot_index: player_slot_index,
        })?
        .ok_or(AssemblerError::EmptySlot {
            slot_index: player_slot_index,
        })?;
    let machine_category =
        assembler_machine_category(&sim.world.prototypes, &sim.entities, entity_id);
    let state = sim.entities.assembler_state(entity_id)?;
    if !assembler_input_can_accept(
        &sim.world.prototypes,
        &sim.research,
        machine_category,
        state,
        stack,
    ) {
        return Err(AssemblerError::InvalidInput(stack.item_id));
    }
    if !state
        .input_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(AssemblerError::InsufficientSpace);
    }

    sim.player_inventory.slots[player_slot_index] = None;
    sim.entities
        .assembler_state_mut(entity_id)?
        .input_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(AssemblerError::from)
}

pub fn assembler_input_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<(), AssemblerError> {
    let stack = {
        let state = sim.entities.assembler_state(entity_id)?;
        stack_in_assembler_inventory_slot(&state.input_inventory, slot_index)?
    };
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(AssemblerError::InsufficientSpace);
    }

    sim.entities
        .assembler_state_mut(entity_id)?
        .input_inventory
        .slots[slot_index] = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(AssemblerError::from)
}

pub fn assembler_output_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<(), AssemblerError> {
    let stack = {
        let state = sim.entities.assembler_state(entity_id)?;
        stack_in_assembler_inventory_slot(&state.output_inventory, slot_index)?
    };
    if !sim
        .player_inventory
        .can_insert(&sim.world.prototypes, stack.item_id, stack.count)
    {
        return Err(AssemblerError::InsufficientSpace);
    }

    sim.entities
        .assembler_state_mut(entity_id)?
        .output_inventory
        .slots[slot_index] = None;
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id, stack.count)
        .map_err(AssemblerError::from)
}

fn player_slot_to_furnace(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<(), FurnaceError> {
    let stack = sim
        .player_inventory()
        .slots
        .get(slot_index)
        .ok_or(FurnaceError::InvalidSlot { slot_index })?
        .ok_or(FurnaceError::EmptySlot { slot_index })?;
    let is_fuel = sim
        .catalog()
        .item(stack.item_id)
        .and_then(|prototype| prototype.fuel_value_joules)
        .is_some();

    if is_fuel {
        player_slot_to_furnace_fuel(sim, entity_id, slot_index)
    } else {
        player_slot_to_furnace_input(sim, entity_id, slot_index)
    }
}
