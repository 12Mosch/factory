use factory_data::EntityKind;
use factory_sim::{MachineStatus, Simulation};

pub fn diagnostic_lines(sim: &Simulation) -> Vec<String> {
    let statuses = sim.machine_statuses();
    statuses
        .total_by_status
        .iter()
        .map(|count| format!("{}: {}", machine_status_name(count.status), count.count))
        .chain(statuses.groups.iter().map(|group| {
            let counts = group
                .counts
                .iter()
                .map(|count| format!("{} {}", count.count, machine_status_name(count.status)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {}", entity_kind_name(group.kind), counts)
        }))
        .collect()
}

pub fn bottleneck_lines(sim: &Simulation) -> Vec<String> {
    sim.bottleneck_hints(5)
        .hints
        .into_iter()
        .map(|hint| hint.message)
        .collect()
}

fn machine_status_name(status: MachineStatus) -> &'static str {
    match status {
        MachineStatus::Working => "Working",
        MachineStatus::Idle => "Idle",
        MachineStatus::NoRecipe => "No recipe",
        MachineStatus::NoResearch => "No research",
        MachineStatus::NoFuel => "No fuel",
        MachineStatus::NoPower => "No power",
        MachineStatus::NoInput => "No input",
        MachineStatus::NoFluid => "No fluid",
        MachineStatus::OutputFull => "Output full",
    }
}

fn entity_kind_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::ResourcePatch => "Resource patches",
        EntityKind::Furnace => "Furnaces",
        EntityKind::MiningDrill => "Mining drills",
        EntityKind::AssemblingMachine => "Assemblers",
        EntityKind::Inserter => "Inserters",
        EntityKind::TransportBelt => "Transport belts",
        EntityKind::Splitter => "Splitters",
        EntityKind::Lab => "Labs",
        EntityKind::Chest => "Chests",
        EntityKind::ElectricPole => "Electric poles",
        EntityKind::SteamEngine => "Steam engines",
        EntityKind::Boiler => "Boilers",
        EntityKind::OffshorePump => "Offshore pumps",
        EntityKind::Pumpjack => "Pumpjacks",
        EntityKind::Pipe => "Pipes",
        EntityKind::StorageTank => "Storage tanks",
    }
}
