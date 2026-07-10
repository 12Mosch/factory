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
    EntityStore::entity_inventory_mut(&mut sim.entities, entity_id)
}

pub fn burner_drill_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&BurnerMiningDrillState, BurnerDrillError> {
    sim.entities.burner_drill_state(entity_id)
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

pub fn lab_state(sim: &Simulation, entity_id: EntityId) -> Result<&LabState, LabError> {
    sim.entities.lab_state(entity_id)
}

pub fn assembler_state(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<&AssemblingMachineState, AssemblerError> {
    sim.entities.assembler_state(entity_id)
}
