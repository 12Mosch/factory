use factory_data::EntityKind;
use factory_sim::{EntityId, Simulation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenMachineKind {
    Chest,
    BurnerDrill,
    Furnace,
    Assembler,
    Lab,
}

pub(crate) fn is_burner_drill_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::BurnerDrill)
}

pub(crate) fn is_furnace_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::Furnace)
}

pub(crate) fn is_assembler_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::Assembler)
}

pub(crate) fn open_machine_kind(sim: &Simulation, entity_id: EntityId) -> Option<OpenMachineKind> {
    let entity = sim.entities().placed_entity(entity_id)?;
    let prototype = sim.catalog().entities.get(entity.prototype_id.index())?;

    if prototype.entity_kind == EntityKind::Chest {
        Some(OpenMachineKind::Chest)
    } else if prototype.entity_kind == EntityKind::MiningDrill
        && sim.burner_drill_state(entity_id).is_ok()
    {
        Some(OpenMachineKind::BurnerDrill)
    } else if prototype.entity_kind == EntityKind::Furnace && sim.furnace_state(entity_id).is_ok() {
        Some(OpenMachineKind::Furnace)
    } else if prototype.entity_kind == EntityKind::AssemblingMachine
        && sim.assembler_state(entity_id).is_ok()
    {
        Some(OpenMachineKind::Assembler)
    } else if prototype.entity_kind == EntityKind::Lab && sim.lab_state(entity_id).is_ok() {
        Some(OpenMachineKind::Lab)
    } else {
        None
    }
}
