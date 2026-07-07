use factory_data::EntityKind;
use factory_sim::{EntityId, Simulation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenMachineKind {
    Chest,
    BurnerDrill,
    Furnace,
    Boiler,
    Assembler,
    Lab,
}

pub(crate) fn is_burner_drill_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::BurnerDrill)
}

pub(crate) fn is_furnace_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::Furnace)
}

pub(crate) fn is_boiler_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::Boiler)
}

pub(crate) fn is_assembler_entity(sim: &Simulation, entity_id: EntityId) -> bool {
    open_machine_kind(sim, entity_id) == Some(OpenMachineKind::Assembler)
}

pub(crate) fn open_machine_kind(sim: &Simulation, entity_id: EntityId) -> Option<OpenMachineKind> {
    match sim.machine_kind(entity_id)? {
        EntityKind::Chest => Some(OpenMachineKind::Chest),
        EntityKind::MiningDrill => Some(OpenMachineKind::BurnerDrill),
        EntityKind::Furnace => Some(OpenMachineKind::Furnace),
        EntityKind::Boiler => Some(OpenMachineKind::Boiler),
        EntityKind::AssemblingMachine => Some(OpenMachineKind::Assembler),
        EntityKind::Lab => Some(OpenMachineKind::Lab),
        _ => None,
    }
}
