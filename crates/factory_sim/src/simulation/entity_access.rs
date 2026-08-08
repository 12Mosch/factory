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

/// Durable stored-energy state of an accumulator, or `None` when `entity_id`
/// is not an accumulator. Accumulators are passive, so this is read-only.
pub fn accumulator_state(sim: &Simulation, entity_id: EntityId) -> Option<&AccumulatorState> {
    sim.entities.accumulators.get(&entity_id)
}

/// Durable scan progress for a radar, or `None` when `entity_id` is not a radar.
pub fn radar_state(sim: &Simulation, entity_id: EntityId) -> Option<&RadarState> {
    sim.entities.radars.get(&entity_id)
}

/// Circuit connector metadata for an entity, or `None` when its prototype
/// declares none. Presentation uses this to decide whether wiring UI applies.
pub fn circuit_connector(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<factory_data::CircuitConnectorPrototype> {
    sim.entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.catalog().entity(placed.prototype_id))
        .and_then(|prototype| prototype.circuit_connector)
}

pub fn constant_combinator_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<&ConstantCombinatorState> {
    sim.entities.constant_combinators.get(&entity_id)
}

pub fn arithmetic_combinator_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<&ArithmeticCombinatorState> {
    sim.entities.arithmetic_combinators.get(&entity_id)
}

pub fn decider_combinator_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<&DeciderCombinatorState> {
    sim.entities.decider_combinators.get(&entity_id)
}

/// Whether a lamp is currently lit. `None` when the entity is not a lamp.
pub fn lamp_is_lit(sim: &Simulation, entity_id: EntityId) -> Option<bool> {
    sim.entities.lamps.get(&entity_id).map(|state| state.lit)
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

pub fn heat_connection_directions(sim: &Simulation, entity_id: EntityId) -> [bool; 4] {
    sim.heat_connection_directions(entity_id)
}

pub fn roboport_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&RoboportState, RoboportError> {
    sim.entities.roboport_state(entity_id)
}

pub fn nuclear_reactor_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&NuclearReactorState, NuclearReactorError> {
    sim.entities.nuclear_reactor_state(entity_id)
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

pub fn rocket_silo_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&RocketSiloState, RocketSiloError> {
    sim.entities.rocket_silo_state(entity_id)
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
        InventoryPanel::NuclearReactorFuel => entity_id
            .and_then(|id| sim.entities.nuclear_reactor_state(id).ok())
            .filter(|_| slot_index == NUCLEAR_REACTOR_FUEL_SLOT_INDEX)
            .and_then(|state| state.energy.fuel_slot.stack()),
        InventoryPanel::NuclearReactorOutput => entity_id
            .and_then(|id| sim.entities.nuclear_reactor_state(id).ok())
            .filter(|_| slot_index == NUCLEAR_REACTOR_OUTPUT_SLOT_INDEX)
            .and_then(|state| state.output_slot.stack()),
        InventoryPanel::RoboportRobots => entity_id
            .and_then(|id| sim.entities.roboport_state(id).ok())
            .and_then(|state| state.robots.slot(slot_index)),
        InventoryPanel::RoboportMaterial => entity_id
            .and_then(|id| sim.entities.roboport_state(id).ok())
            .and_then(|state| state.materials.slot(slot_index)),
        InventoryPanel::InserterFuel => entity_id
            .and_then(|id| sim.entities.inserter_energy(id).ok())
            .filter(|_| slot_index == INSERTER_FUEL_SLOT_INDEX)
            .and_then(MachineEnergy::fuel_slot)
            .and_then(|slot| slot.stack()),
        // Rolling stock is not a placed entity, so an entity-keyed lookup has
        // nothing to answer with; the wagon window reads
        // `rolling_stock_panel_slot` instead.
        InventoryPanel::RollingStockCargo | InventoryPanel::RollingStockFuel => None,
        InventoryPanel::AssemblerInput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .and_then(|state| state.input_inventory.slot(slot_index)),
        InventoryPanel::AssemblerOutput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .and_then(|state| state.output_inventory.slot(slot_index)),
        InventoryPanel::RocketSiloInput => entity_id
            .and_then(|id| sim.entities.rocket_silo_state(id).ok())
            .and_then(|state| state.input_inventory.slot(slot_index)),
        InventoryPanel::RocketSiloCargo => entity_id
            .and_then(|id| sim.entities.rocket_silo_state(id).ok())
            .and_then(|state| state.cargo_inventory.slot(slot_index)),
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
        InventoryPanel::RollingStockCargo | InventoryPanel::RollingStockFuel => 0,
        InventoryPanel::BoilerFuel => entity_id
            .and_then(|id| sim.entities.boiler_state(id).ok())
            .map_or(0, |_| 1),
        InventoryPanel::NuclearReactorFuel | InventoryPanel::NuclearReactorOutput => entity_id
            .and_then(|id| sim.entities.nuclear_reactor_state(id).ok())
            .map_or(0, |_| 1),
        InventoryPanel::RoboportRobots => entity_id
            .and_then(|id| sim.entities.roboport_state(id).ok())
            .map_or(0, |state| state.robots.slots().len()),
        InventoryPanel::RoboportMaterial => entity_id
            .and_then(|id| sim.entities.roboport_state(id).ok())
            .map_or(0, |state| state.materials.slots().len()),
        InventoryPanel::InserterFuel => entity_id
            .and_then(|id| sim.entities.inserter_energy(id).ok())
            .map_or(0, |energy| usize::from(energy.fuel_slot().is_some())),
        InventoryPanel::AssemblerInput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .map_or(0, |state| state.input_inventory.slots().len()),
        InventoryPanel::AssemblerOutput => entity_id
            .and_then(|id| sim.entities.assembler_state(id).ok())
            .map_or(0, |state| state.output_inventory.slots().len()),
        InventoryPanel::RocketSiloInput => entity_id
            .and_then(|id| sim.entities.rocket_silo_state(id).ok())
            .map_or(0, |state| state.input_inventory.slots().len()),
        InventoryPanel::RocketSiloCargo => entity_id
            .and_then(|id| sim.entities.rocket_silo_state(id).ok())
            .map_or(0, |state| state.cargo_inventory.slots().len()),
        InventoryPanel::Modules => entity_id
            .and_then(|id| module_slots(sim, id).ok())
            .map_or(0, ModuleSlots::len),
    }
}

/// What a rolling-stock window shows in one slot.
///
/// The stock counterpart of [`inventory_panel_slot`]: the wagon window draws
/// the player's inventory beside a piece of rolling stock, so `Player` answers
/// the same way it does there and the two stock panels answer from the piece.
pub fn rolling_stock_panel_slot(
    sim: &Simulation,
    stock_id: Option<RollingStockId>,
    panel: InventoryPanel,
    slot_index: usize,
) -> Option<ItemStack> {
    match panel {
        InventoryPanel::Player => sim.player_inventory.slot(slot_index),
        InventoryPanel::RollingStockCargo => sim
            .rolling_stock_piece(stock_id?)?
            .inventory
            .as_ref()?
            .slot(slot_index),
        InventoryPanel::RollingStockFuel => sim
            .rolling_stock_piece(stock_id?)?
            .energy
            .as_ref()
            .filter(|_| slot_index == ROLLING_STOCK_FUEL_SLOT_INDEX)?
            .fuel_slot
            .stack(),
        _ => None,
    }
}

/// The filter set on one cargo slot of a piece of rolling stock, if any.
pub fn rolling_stock_slot_filter(
    sim: &Simulation,
    stock_id: RollingStockId,
    slot_index: usize,
) -> Option<ItemId> {
    sim.rolling_stock_piece(stock_id)?
        .inventory
        .as_ref()?
        .filter(slot_index)
}

pub fn rolling_stock_panel_slot_count(
    sim: &Simulation,
    stock_id: Option<RollingStockId>,
    panel: InventoryPanel,
) -> usize {
    let Some(stock_id) = stock_id else {
        return 0;
    };
    match panel {
        InventoryPanel::Player => sim.player_inventory.slots().len(),
        InventoryPanel::RollingStockCargo => sim
            .rolling_stock_piece(stock_id)
            .and_then(|stock| stock.inventory.as_ref())
            .map_or(0, |inventory| inventory.slots().len()),
        InventoryPanel::RollingStockFuel => sim
            .rolling_stock_piece(stock_id)
            .map_or(0, |stock| usize::from(stock.energy.is_some())),
        _ => 0,
    }
}
