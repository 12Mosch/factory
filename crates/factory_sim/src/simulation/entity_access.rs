use super::*;

/// The machine kind backing `entity_id`, derived from which state map owns it.
/// `None` when the entity does not exist or carries no per-kind machine state.
pub fn machine_kind(sim: &Simulation, entity_id: EntityId) -> Option<EntityKind> {
    sim.entities.machine_kind(entity_id)
}

pub fn inventory(sim: &Simulation, entity_id: EntityId) -> Result<&Inventory, ContainerError> {
    EntityStore::entity_inventory(&sim.entities, entity_id)
}

pub fn inventory_mut(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<&mut Inventory, ContainerError> {
    EntityStore::entity_inventory(&sim.entities, entity_id)?;
    sim.invalidate_consumer_power_demand(entity_id);
    EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)
}

pub fn mining_drill_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&MiningDrillState, MiningDrillError> {
    sim.entities.mining_drill_state(entity_id)
}

pub fn furnace_state(sim: &Simulation, entity_id: EntityId) -> Result<&FurnaceState, FurnaceError> {
    sim.entities.furnace_state(entity_id)
}

pub fn boiler_state(sim: &Simulation, entity_id: EntityId) -> Result<&BoilerState, BoilerError> {
    sim.entities.boiler_state(entity_id)
}

pub fn fluid_box_states(sim: &Simulation, entity_id: EntityId) -> Option<&[FluidBoxState]> {
    sim.entities.fluid_box_states(entity_id)
}

/// For each cardinal direction (indexed by [`Direction::index`]), whether `entity_id` has a
/// fluid connection joined to a matching connection on the adjacent entity. All false when
/// the entity does not exist or has no fluid boxes.
pub fn fluid_connection_directions(sim: &Simulation, entity_id: EntityId) -> [bool; 4] {
    sim.fluid_connection_directions(entity_id)
}

pub fn belt_segment(sim: &Simulation, entity_id: EntityId) -> Result<&BeltSegment, BeltError> {
    sim.entities.belt_segment(entity_id)
}

pub fn splitter_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&SplitterState, SplitterError> {
    sim.entities.splitter_state(entity_id)
}

pub fn inserter_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&InserterState, InserterError> {
    sim.entities.inserter_state(entity_id)
}

pub fn inserter_energy(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&MachineEnergy, InserterError> {
    sim.entities.inserter_energy(entity_id)
}

pub fn lab_state(sim: &Simulation, entity_id: EntityId) -> Result<&LabState, LabError> {
    sim.entities.lab_state(entity_id)
}

pub fn assembler_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&AssemblingMachineState, AssemblerError> {
    sim.entities.assembler_state(entity_id)
}

pub fn module_slots(sim: &Simulation, entity_id: EntityId) -> Result<&ModuleSlots, ModuleError> {
    if let Some(slots) = sim.entities.module_slots(entity_id) {
        Ok(slots)
    } else if sim.entities.placed_entity(entity_id).is_some() {
        Err(ModuleError::UnsupportedMachine(entity_id))
    } else {
        Err(ModuleError::MissingEntity(entity_id))
    }
}

pub fn resolved_module_effects(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<ResolvedModuleEffects, ModuleError> {
    if let Some(modules) = sim.entities.machine_module_state(entity_id) {
        Ok(modules.resolved_effects)
    } else if let Some(state) = sim.entities.beacons.get(&entity_id) {
        let transmission = sim
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.beacon)
            .map_or(0, |beacon| beacon.transmission_permyriad);
        let mut effects = ResolvedModuleEffects::default();
        for stack in state.slots.slots().iter().filter_map(|slot| slot.stack()) {
            if let Some(effect) = sim
                .world
                .prototypes
                .item(stack.item_id())
                .and_then(|item| item.module_effect)
            {
                effects.add_effect(effect, transmission);
            }
        }
        Ok(effects)
    } else if sim.entities.placed_entity(entity_id).is_some() {
        Err(ModuleError::UnsupportedMachine(entity_id))
    } else {
        Err(ModuleError::MissingEntity(entity_id))
    }
}

pub fn productivity_progress_permyriad(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<u32, ModuleError> {
    if let Some(modules) = sim.entities.machine_module_state(entity_id) {
        Ok(modules.productivity_progress_permyriad)
    } else if sim.entities.placed_entity(entity_id).is_some() {
        Err(ModuleError::UnsupportedMachine(entity_id))
    } else {
        Err(ModuleError::MissingEntity(entity_id))
    }
}

/// Resolves a displayed inventory panel slot without exposing the entity
/// state's storage layout to presentation code.
pub fn inventory_panel_slot(
    sim: &Simulation,
    entity_id: Option<EntityId>,
    panel: InventoryPanel,
    slot_index: usize,
) -> Option<ItemStack> {
    match panel {
        InventoryPanel::Player => sim.player_inventory.slot(slot_index),
        InventoryPanel::Container => entity_id
            .and_then(|id| EntityStore::entity_inventory(&sim.entities, id).ok())
            .and_then(|inventory| inventory.slot(slot_index)),
        InventoryPanel::BurnerFuel => entity_id
            .and_then(|id| sim.entities.mining_drill_state(id).ok())
            .filter(|_| slot_index == MINING_DRILL_FUEL_SLOT_INDEX)
            .and_then(|state| state.energy.fuel_slot())
            .and_then(|slot| slot.stack()),
        InventoryPanel::BurnerOutput => entity_id
            .and_then(|id| sim.entities.mining_drill_state(id).ok())
            .filter(|_| slot_index == MINING_DRILL_OUTPUT_SLOT_INDEX)
            .and_then(|state| state.output_slot.stack()),
        InventoryPanel::FurnaceInput => entity_id
            .and_then(|id| sim.entities.furnace_state(id).ok())
            .filter(|_| slot_index == FURNACE_INPUT_SLOT_INDEX)
            .and_then(|state| state.input_slot.stack()),
        InventoryPanel::FurnaceFuel => entity_id
            .and_then(|id| sim.entities.furnace_state(id).ok())
            .filter(|_| slot_index == FURNACE_FUEL_SLOT_INDEX)
            .and_then(|state| state.energy.fuel_slot())
            .and_then(|slot| slot.stack()),
        InventoryPanel::FurnaceOutput => entity_id
            .and_then(|id| sim.entities.furnace_state(id).ok())
            .filter(|_| slot_index == FURNACE_OUTPUT_SLOT_INDEX)
            .and_then(|state| state.output_slot.stack()),
        InventoryPanel::BoilerFuel => entity_id
            .and_then(|id| sim.entities.boiler_state(id).ok())
            .filter(|_| slot_index == BOILER_FUEL_SLOT_INDEX)
            .and_then(|state| state.energy.fuel_slot.stack()),
        InventoryPanel::InserterFuel => entity_id
            .and_then(|id| sim.entities.inserter_energy(id).ok())
            .filter(|_| slot_index == INSERTER_FUEL_SLOT_INDEX)
            .and_then(MachineEnergy::fuel_slot)
            .and_then(|slot| slot.stack()),
        InventoryPanel::AssemblerInput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .and_then(|state| state.input_inventory.slot(slot_index)),
        InventoryPanel::AssemblerOutput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .and_then(|state| state.output_inventory.slot(slot_index)),
        InventoryPanel::Modules => entity_id
            .and_then(|id| module_slots(sim, id).ok())
            .and_then(|slots| slots.slot(slot_index)),
    }
}

/// Number of slots represented by a displayed inventory panel.
pub fn inventory_panel_slot_count(
    sim: &Simulation,
    entity_id: Option<EntityId>,
    panel: InventoryPanel,
) -> usize {
    match panel {
        InventoryPanel::Player => sim.player_inventory.slots().len(),
        InventoryPanel::Container => entity_id
            .and_then(|id| EntityStore::entity_inventory(&sim.entities, id).ok())
            .map_or(0, |inventory| inventory.slots().len()),
        InventoryPanel::BurnerFuel => entity_id
            .and_then(|id| sim.entities.mining_drill_state(id).ok())
            .map_or(0, |state| usize::from(state.energy.fuel_slot().is_some())),
        InventoryPanel::BurnerOutput => entity_id
            .and_then(|id| sim.entities.mining_drill_state(id).ok())
            .map_or(0, |_| 1),
        InventoryPanel::FurnaceFuel => entity_id
            .and_then(|id| sim.entities.furnace_state(id).ok())
            .map_or(0, |state| usize::from(state.energy.fuel_slot().is_some())),
        InventoryPanel::FurnaceInput | InventoryPanel::FurnaceOutput => entity_id
            .and_then(|id| sim.entities.furnace_state(id).ok())
            .map_or(0, |_| 1),
        InventoryPanel::BoilerFuel => entity_id
            .and_then(|id| sim.entities.boiler_state(id).ok())
            .map_or(0, |_| 1),
        InventoryPanel::InserterFuel => entity_id
            .and_then(|id| sim.entities.inserter_energy(id).ok())
            .map_or(0, |energy| usize::from(energy.fuel_slot().is_some())),
        InventoryPanel::AssemblerInput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .map_or(0, |state| state.input_inventory.slots().len()),
        InventoryPanel::AssemblerOutput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .map_or(0, |state| state.output_inventory.slots().len()),
        InventoryPanel::Modules => entity_id
            .and_then(|id| module_slots(sim, id).ok())
            .map_or(0, ModuleSlots::len),
    }
}
