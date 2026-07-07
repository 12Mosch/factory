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
