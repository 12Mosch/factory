use factory_sim::{
    AssemblerError, BoilerError, BurnerDrillError, ContainerError, EntityId, FurnaceError,
    Simulation,
};

use crate::interaction::machine_kind::{
    is_assembler_entity, is_boiler_entity, is_burner_drill_entity, is_furnace_entity,
};
use crate::ui::inventory_panel::InventoryPanel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerSlotClickError {
    NoOpenContainer,
    Transfer(ContainerError),
    BurnerDrill(BurnerDrillError),
    Furnace(FurnaceError),
    Boiler(BoilerError),
    Assembler(AssemblerError),
}

pub fn transfer_open_container_slot(
    sim: &mut Simulation,
    open_entity_id: Option<EntityId>,
    panel: InventoryPanel,
    slot_index: usize,
) -> Result<(), ContainerSlotClickError> {
    let entity_id = open_entity_id.ok_or(ContainerSlotClickError::NoOpenContainer)?;

    match panel {
        InventoryPanel::Player => {
            if is_burner_drill_entity(sim, entity_id) {
                return sim
                    .transfer_player_slot_to_burner_drill_fuel(entity_id, slot_index)
                    .map_err(ContainerSlotClickError::BurnerDrill);
            }
            if is_furnace_entity(sim, entity_id) {
                return transfer_player_slot_to_furnace(sim, entity_id, slot_index)
                    .map_err(ContainerSlotClickError::Furnace);
            }
            if is_boiler_entity(sim, entity_id) {
                return sim
                    .transfer_player_slot_to_boiler_fuel(entity_id, slot_index)
                    .map_err(ContainerSlotClickError::Boiler);
            }
            if is_assembler_entity(sim, entity_id) {
                return sim
                    .transfer_player_slot_to_assembler_input(entity_id, slot_index)
                    .map_err(ContainerSlotClickError::Assembler);
            }
            sim.transfer_player_slot_to_entity(entity_id, slot_index)
        }
        InventoryPanel::Container => sim.transfer_entity_slot_to_player(entity_id, slot_index),
        InventoryPanel::BurnerFuel => {
            return sim
                .transfer_burner_drill_fuel_to_player(entity_id)
                .map_err(ContainerSlotClickError::BurnerDrill);
        }
        InventoryPanel::BurnerOutput => {
            return sim
                .transfer_burner_drill_output_to_player(entity_id)
                .map_err(ContainerSlotClickError::BurnerDrill);
        }
        InventoryPanel::FurnaceInput => {
            return sim
                .transfer_furnace_input_to_player(entity_id)
                .map_err(ContainerSlotClickError::Furnace);
        }
        InventoryPanel::FurnaceFuel => {
            return sim
                .transfer_furnace_fuel_to_player(entity_id)
                .map_err(ContainerSlotClickError::Furnace);
        }
        InventoryPanel::FurnaceOutput => {
            return sim
                .transfer_furnace_output_to_player(entity_id)
                .map_err(ContainerSlotClickError::Furnace);
        }
        InventoryPanel::BoilerFuel => {
            return sim
                .transfer_boiler_fuel_to_player(entity_id)
                .map_err(ContainerSlotClickError::Boiler);
        }
        InventoryPanel::AssemblerInput => {
            return sim
                .transfer_assembler_input_slot_to_player(entity_id, slot_index)
                .map_err(ContainerSlotClickError::Assembler);
        }
        InventoryPanel::AssemblerOutput => {
            return sim
                .transfer_assembler_output_slot_to_player(entity_id, slot_index)
                .map_err(ContainerSlotClickError::Assembler);
        }
    }
    .map_err(ContainerSlotClickError::Transfer)
}

pub(crate) fn transfer_player_slot_to_furnace(
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
        sim.transfer_player_slot_to_furnace_fuel(entity_id, slot_index)
    } else {
        sim.transfer_player_slot_to_furnace_input(entity_id, slot_index)
    }
}
