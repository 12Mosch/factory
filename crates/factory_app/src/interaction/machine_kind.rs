use factory_data::EntityKind;
use factory_sim::{EntityId, Simulation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenMachineKind {
    Chest,
    MiningDrill,
    Furnace,
    Boiler,
    Assembler,
    Lab,
    Turret,
    Inserter,
    Beacon,
}

pub(crate) fn open_machine_kind(sim: &Simulation, entity_id: EntityId) -> Option<OpenMachineKind> {
    match factory_sim::entity_access::machine_kind(sim, entity_id)? {
        EntityKind::Chest => Some(OpenMachineKind::Chest),
        EntityKind::MiningDrill => Some(OpenMachineKind::MiningDrill),
        EntityKind::Furnace => Some(OpenMachineKind::Furnace),
        EntityKind::Boiler => Some(OpenMachineKind::Boiler),
        EntityKind::AssemblingMachine => Some(OpenMachineKind::Assembler),
        EntityKind::Lab => Some(OpenMachineKind::Lab),
        EntityKind::Beacon => Some(OpenMachineKind::Beacon),
        EntityKind::GunTurret => Some(OpenMachineKind::Turret),
        EntityKind::Inserter => sim
            .entities()
            .placed_entity(entity_id)
            .and_then(|placed| sim.catalog().entity(placed.prototype_id))
            .and_then(|prototype| prototype.burner.as_ref())
            .map(|_| OpenMachineKind::Inserter),
        _ => None,
    }
}
