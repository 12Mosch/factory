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
    NuclearReactor,
    Roboport,
    /// A heat network entity with nothing to configure (heat pipe, heat
    /// exchanger). Opening it shows its temperature, which is what explains a
    /// heat network that is not yet making steam.
    HeatBuffer,
    ConstantCombinator,
    ArithmeticCombinator,
    DeciderCombinator,
    /// An entity whose only configurable surface is its circuit connector
    /// (belts, pumps, tanks, accumulators, lamps). Without this the player
    /// would have no way to reach their conditions.
    Circuit,
}

pub(crate) fn open_machine_kind(sim: &Simulation, entity_id: EntityId) -> Option<OpenMachineKind> {
    let kind = factory_sim::entity_access::machine_kind(sim, entity_id)?;
    let machine_window = match kind {
        EntityKind::Chest => Some(OpenMachineKind::Chest),
        EntityKind::MiningDrill => Some(OpenMachineKind::MiningDrill),
        EntityKind::Furnace => Some(OpenMachineKind::Furnace),
        EntityKind::Boiler => Some(OpenMachineKind::Boiler),
        EntityKind::AssemblingMachine => Some(OpenMachineKind::Assembler),
        EntityKind::Lab => Some(OpenMachineKind::Lab),
        EntityKind::Beacon => Some(OpenMachineKind::Beacon),
        EntityKind::NuclearReactor => Some(OpenMachineKind::NuclearReactor),
        EntityKind::Roboport => Some(OpenMachineKind::Roboport),
        EntityKind::HeatPipe | EntityKind::HeatExchanger => Some(OpenMachineKind::HeatBuffer),
        EntityKind::GunTurret => Some(OpenMachineKind::Turret),
        EntityKind::ConstantCombinator => Some(OpenMachineKind::ConstantCombinator),
        EntityKind::ArithmeticCombinator => Some(OpenMachineKind::ArithmeticCombinator),
        EntityKind::DeciderCombinator => Some(OpenMachineKind::DeciderCombinator),
        EntityKind::Inserter => sim
            .entities()
            .placed_entity(entity_id)
            .and_then(|placed| sim.catalog().entity(placed.prototype_id))
            .and_then(|prototype| prototype.burner.as_ref())
            .map(|_| OpenMachineKind::Inserter),
        _ => None,
    };
    machine_window.or_else(|| {
        factory_sim::entity_access::circuit_connector(sim, entity_id)
            .map(|_| OpenMachineKind::Circuit)
    })
}
