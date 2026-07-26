use bevy::prelude::*;
use factory_sim::{
    BOILER_FUEL_SLOT_INDEX, FURNACE_FUEL_SLOT_INDEX, FURNACE_INPUT_SLOT_INDEX,
    FURNACE_OUTPUT_SLOT_INDEX, INSERTER_FUEL_SLOT_INDEX, MINING_DRILL_FUEL_SLOT_INDEX,
    MINING_DRILL_OUTPUT_SLOT_INDEX, MachineStatus, NUCLEAR_REACTOR_FUEL_SLOT_INDEX,
    NUCLEAR_REACTOR_OUTPUT_SLOT_INDEX,
};

use crate::constants::{MACHINE_BAR_HEIGHT, MACHINE_BAR_WIDTH};
use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::SimResource;
use crate::ui::inventory_panel::{InventoryPanel, spawn_labeled_slot, spawn_slot_button};
use crate::ui::resources::OpenContainer;

#[derive(Component)]
pub(crate) struct BurnerEnergyText;

#[derive(Component)]
pub(crate) struct MachineProgressFill;

/// Heat-buffer temperature readout. A heat network's whole behaviour hangs on
/// temperature, so reactors, pipes, and exchangers all show theirs.
#[derive(Component)]
pub(crate) struct HeatTemperatureText;

/// What one live line of the roboport panel reports.
///
/// Keyed like [`crate::ui::logistics_panel::LogisticLabel`] and for the same
/// reason: every line writes `Text`, so one component with a discriminant keeps
/// the refresh to a single query however many readouts the panel grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoboportReadout {
    /// How full the charging buffer is, which is what tells a player whether
    /// the roboport is powered.
    Charge,
    ConstructionRobots,
    LogisticRobots,
    Jobs,
    /// Everything the network's logistic chests hold.
    NetworkContents,
    /// Requests the network has not been able to fill, which is the first thing
    /// to look at when a requester chest stays empty.
    UnsatisfiedRequests,
}

#[derive(Component)]
pub(crate) struct RoboportReadoutText(pub(crate) RoboportReadout);

/// Item lines one network list shows before summarizing the rest.
const NETWORK_ITEM_LINE_LIMIT: usize = 8;

const PRIMARY_READOUT_COLOR: Color = Color::srgb(0.86, 0.88, 0.82);
const SECONDARY_READOUT_COLOR: Color = Color::srgb(0.70, 0.76, 0.72);
/// Unfilled requests are the network failing at its job, so they read as a
/// warning rather than as another grey line.
const SHORTFALL_READOUT_COLOR: Color = Color::srgb(1.0, 0.72, 0.30);

#[derive(Component)]
pub(crate) struct MachineGuidanceText;

pub(crate) fn spawn_machine_guidance(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    status: MachineStatus,
) {
    root.spawn((
        Text::new(format_machine_guidance(status)),
        TextFont::from_font_size(11.0),
        TextColor(machine_guidance_color(status)),
        TextLayout::justify(Justify::Left),
        Node {
            width: Val::Percent(100.0),
            ..default()
        },
        MachineGuidanceText,
    ));
}

pub(crate) fn format_machine_guidance(status: MachineStatus) -> &'static str {
    match status {
        MachineStatus::Working => "Working — machine is operating normally.",
        MachineStatus::Idle => "Idle — give this machine work to do.",
        MachineStatus::NoRecipe => "Missing recipe — select a recipe above to begin crafting.",
        MachineStatus::NoResearch => {
            "No research — select research or unlock the required technology."
        }
        MachineStatus::NoFuel => "Needs fuel — add a burnable item to the Fuel slot.",
        MachineStatus::NoPower => "No power — connect the machine to a powered electric network.",
        MachineStatus::NoInput => "Missing input — add the required ingredients or resources.",
        MachineStatus::NoFluid => "Missing fluid — connect a pipe carrying the required fluid.",
        MachineStatus::NoHeat => {
            "Too cold — the heat network needs more reactor output or time to warm up."
        }
        MachineStatus::OutputFull => {
            "Output blocked — clear the output or connect space for products."
        }
    }
}

fn machine_guidance_color(status: MachineStatus) -> Color {
    match status {
        MachineStatus::Working => Color::srgb(0.42, 0.84, 0.55),
        MachineStatus::Idle => Color::srgb(0.72, 0.74, 0.72),
        MachineStatus::NoRecipe => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoResearch => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoFuel => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoPower => Color::srgb(1.0, 0.52, 0.30),
        MachineStatus::NoInput => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoFluid => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::NoHeat => Color::srgb(1.0, 0.72, 0.30),
        MachineStatus::OutputFull => Color::srgb(1.0, 0.72, 0.30),
    }
}

pub(crate) fn update_machine_guidance(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut guidance: Query<(&mut Text, &mut TextColor), With<MachineGuidanceText>>,
) {
    let status = open_container
        .entity_id
        .and_then(|entity_id| sim.read().machine_status_for_entity(entity_id));
    let Some(status) = status else {
        return;
    };

    for (mut text, mut color) in &mut guidance {
        text.0 = format_machine_guidance(status).to_string();
        color.0 = machine_guidance_color(status);
    }
}

pub(crate) fn spawn_mining_drill_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    entity_id: factory_sim::EntityId,
) {
    let title = machine_panel_title(sim, entity_id, "Mining Drill");
    let has_fuel_slot = factory_sim::entity_access::inventory_panel_slot_count(
        sim,
        Some(entity_id),
        InventoryPanel::BurnerFuel,
    ) > 0;
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new(title),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        if has_fuel_slot {
            panel.spawn((
                Text::new("Energy: 0 J"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.86, 0.88, 0.82)),
                BurnerEnergyText,
            ));
        }
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.33, 0.74, 0.48)),
                MachineProgressFill,
            ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                if has_fuel_slot {
                    spawn_labeled_slot(
                        slots,
                        "Fuel",
                        InventoryPanel::BurnerFuel,
                        MINING_DRILL_FUEL_SLOT_INDEX,
                    );
                }
                spawn_labeled_slot(
                    slots,
                    "Output",
                    InventoryPanel::BurnerOutput,
                    MINING_DRILL_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

/// Panel title from the open entity's prototype name, e.g. "Electric
/// Furnace" for `electric_furnace`.
fn machine_panel_title(
    sim: &factory_sim::Simulation,
    entity_id: factory_sim::EntityId,
    fallback: &str,
) -> String {
    sim.entities()
        .placed_entity(entity_id)
        .and_then(|placed| sim.catalog().entity(placed.prototype_id))
        .map(|prototype| crate::ui::formatting::format_recipe_display_name(&prototype.name))
        .unwrap_or_else(|| fallback.to_string())
}

pub(crate) fn spawn_furnace_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    entity_id: factory_sim::EntityId,
) {
    let title = machine_panel_title(sim, entity_id, "Furnace");
    let has_fuel_slot = factory_sim::entity_access::inventory_panel_slot_count(
        sim,
        Some(entity_id),
        InventoryPanel::FurnaceFuel,
    ) > 0;
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new(title),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        if has_fuel_slot {
            panel.spawn((
                Text::new("Energy: 0 J"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.86, 0.88, 0.82)),
                BurnerEnergyText,
            ));
        }
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.82, 0.48, 0.24)),
                MachineProgressFill,
            ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                spawn_labeled_slot(
                    slots,
                    "Input",
                    InventoryPanel::FurnaceInput,
                    FURNACE_INPUT_SLOT_INDEX,
                );
                if has_fuel_slot {
                    spawn_labeled_slot(
                        slots,
                        "Fuel",
                        InventoryPanel::FurnaceFuel,
                        FURNACE_FUEL_SLOT_INDEX,
                    );
                }
                spawn_labeled_slot(
                    slots,
                    "Output",
                    InventoryPanel::FurnaceOutput,
                    FURNACE_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

pub(crate) fn spawn_boiler_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Boiler"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                spawn_labeled_slot(
                    slots,
                    "Fuel",
                    InventoryPanel::BoilerFuel,
                    BOILER_FUEL_SLOT_INDEX,
                );
            });
    });
}

pub(crate) fn spawn_nuclear_reactor_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    entity_id: factory_sim::EntityId,
) {
    let title = machine_panel_title(sim, entity_id, "Nuclear Reactor");
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new(title),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        spawn_heat_temperature_text(panel);
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        panel
            .spawn((
                Node {
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|slots| {
                spawn_labeled_slot(
                    slots,
                    "Fuel",
                    InventoryPanel::NuclearReactorFuel,
                    NUCLEAR_REACTOR_FUEL_SLOT_INDEX,
                );
                spawn_labeled_slot(
                    slots,
                    "Spent",
                    InventoryPanel::NuclearReactorOutput,
                    NUCLEAR_REACTOR_OUTPUT_SLOT_INDEX,
                );
            });
    });
}

/// Panel for heat entities with nothing to configure (heat pipes, heat
/// exchangers). The temperature alone is what a player needs to diagnose why a
/// heat network is not making steam.
pub(crate) fn spawn_heat_buffer_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    entity_id: factory_sim::EntityId,
) {
    let title = machine_panel_title(sim, entity_id, "Heat Buffer");
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new(title),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        spawn_heat_temperature_text(panel);
    });
}

/// Roboport panel: charging buffer, coverage radii, and the two inventories.
///
/// The radii are shown as numbers here and drawn as squares in the world
/// overlay; the panel is where a player checks them against a plan, the overlay
/// is where they check them against the terrain.
pub(crate) fn spawn_roboport_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    entity_id: factory_sim::EntityId,
) {
    let title = machine_panel_title(sim, entity_id, "Roboport");
    let coverage = sim
        .entities()
        .placed_entity(entity_id)
        .and_then(|placed| sim.catalog().entity(placed.prototype_id))
        .and_then(|prototype| prototype.roboport);
    let robot_slots = factory_sim::entity_access::inventory_panel_slot_count(
        sim,
        Some(entity_id),
        InventoryPanel::RoboportRobots,
    );
    let material_slots = factory_sim::entity_access::inventory_panel_slot_count(
        sim,
        Some(entity_id),
        InventoryPanel::RoboportMaterial,
    );

    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(240.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new(title),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        spawn_roboport_readout(panel, RoboportReadout::Charge, 12.0, PRIMARY_READOUT_COLOR);
        spawn_roboport_readout(
            panel,
            RoboportReadout::ConstructionRobots,
            12.0,
            PRIMARY_READOUT_COLOR,
        );
        spawn_roboport_readout(
            panel,
            RoboportReadout::LogisticRobots,
            12.0,
            PRIMARY_READOUT_COLOR,
        );
        spawn_roboport_readout(panel, RoboportReadout::Jobs, 11.0, SECONDARY_READOUT_COLOR);
        if let Some(coverage) = coverage {
            panel.spawn((
                Text::new(format!(
                    "Construction {} tiles · Logistic {} tiles",
                    coverage.construction_radius_tiles, coverage.logistic_radius_tiles
                )),
                TextFont::from_font_size(11.0),
                TextColor(SECONDARY_READOUT_COLOR),
            ));
        }
        // The network's own state, below the roboport's. Contents answer "does
        // the network have this at all" and unsatisfied requests answer "why is
        // that chest still empty" — the two questions a player opens a roboport
        // to ask once robots are flying.
        spawn_roboport_readout(
            panel,
            RoboportReadout::NetworkContents,
            11.0,
            SECONDARY_READOUT_COLOR,
        );
        spawn_roboport_readout(
            panel,
            RoboportReadout::UnsatisfiedRequests,
            11.0,
            SHORTFALL_READOUT_COLOR,
        );
        spawn_roboport_slot_row(panel, "Robots", InventoryPanel::RoboportRobots, robot_slots);
        spawn_roboport_slot_row(
            panel,
            "Material",
            InventoryPanel::RoboportMaterial,
            material_slots,
        );
    });
}

fn spawn_roboport_readout(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    readout: RoboportReadout,
    font_size: f32,
    color: Color,
) {
    panel.spawn((
        Text::new(roboport_readout_text(readout, None, None)),
        TextFont::from_font_size(font_size),
        TextColor(color),
        Node {
            width: Val::Percent(100.0),
            ..default()
        },
        RoboportReadoutText(readout),
    ));
}

/// Text of one roboport readout.
///
/// Shared by the spawn and the refresh so a panel that has never been updated
/// shows the same shape it will show a frame later, rather than a placeholder
/// that has to be kept in step with the real formatting by hand.
fn roboport_readout_text(
    readout: RoboportReadout,
    status: Option<factory_sim::EntityRoboportStatus>,
    sim: Option<&factory_sim::Simulation>,
) -> String {
    match readout {
        RoboportReadout::Charge => match status {
            Some(status) if status.charge_capacity_joules > 0 => format!(
                "Charge: {:.0}% ({:.1} MJ)",
                status.charge_energy_joules as f64 * 100.0 / status.charge_capacity_joules as f64,
                status.charge_energy_joules as f64 / 1_000_000.0,
            ),
            _ => "Charge: -".to_string(),
        },
        RoboportReadout::ConstructionRobots => format!(
            "Construction robots: {} / {}",
            status.map_or(0, |status| status.available_construction_robots),
            status.map_or(0, |status| status.total_construction_robots),
        ),
        RoboportReadout::LogisticRobots => format!(
            "Logistic robots: {} / {} · {} delivering",
            status.map_or(0, |status| status.available_logistic_robots),
            status.map_or(0, |status| status.total_logistic_robots),
            status.map_or(0, |status| status.active_deliveries),
        ),
        RoboportReadout::Jobs => {
            let jobs = status.map(|status| status.jobs).unwrap_or_default();
            format!(
                "Jobs: Build {} · Deconstruct {} · Repair {}",
                jobs.build, jobs.deconstruction, jobs.repair
            )
        }
        RoboportReadout::NetworkContents => {
            format_network_items(sim, status, NetworkItemReport::Contents)
        }
        RoboportReadout::UnsatisfiedRequests => {
            format_network_items(sim, status, NetworkItemReport::UnsatisfiedRequests)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NetworkItemReport {
    Contents,
    UnsatisfiedRequests,
}

/// Items a network holds, or the requests it has not filled.
///
/// Both lists are cut off at [`NETWORK_ITEM_LINE_LIMIT`] entries, ordered by
/// amount so the cut always drops the least interesting ones: a network with
/// four hundred distinct items would otherwise turn the panel into a wall of
/// text nobody reads.
fn format_network_items(
    sim: Option<&factory_sim::Simulation>,
    status: Option<factory_sim::EntityRoboportStatus>,
    report: NetworkItemReport,
) -> String {
    let heading = match report {
        NetworkItemReport::Contents => "Network contents",
        NetworkItemReport::UnsatisfiedRequests => "Unsatisfied requests",
    };
    let Some((sim, network_id)) = sim.zip(status.and_then(|status| status.network_id)) else {
        return format!("{heading}: -");
    };
    let Some(contents) = sim.logistic_network_contents(network_id) else {
        return format!("{heading}: -");
    };

    let mut entries = contents
        .iter()
        .filter_map(|(item_id, totals)| {
            let amount = match report {
                NetworkItemReport::Contents => totals.stored,
                NetworkItemReport::UnsatisfiedRequests => totals.requested,
            };
            (amount > 0).then_some((amount, *item_id))
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return match report {
            NetworkItemReport::Contents => format!("{heading}: empty"),
            NetworkItemReport::UnsatisfiedRequests => format!("{heading}: none"),
        };
    }
    // Largest first, ties by item id so the list never reshuffles under a
    // player reading it.
    entries.sort_unstable_by(|left, right| right.cmp(left));

    let shown = entries.len().min(NETWORK_ITEM_LINE_LIMIT);
    let mut text = format!("{heading}:");
    for (amount, item_id) in &entries[..shown] {
        let name = sim
            .catalog()
            .item(*item_id)
            .map_or("unknown", |item| item.name.as_str());
        text.push_str(&format!("\n  {name} × {amount}"));
    }
    if let Some(hidden) = entries.len().checked_sub(shown).filter(|count| *count > 0) {
        text.push_str(&format!("\n  … {hidden} more"));
    }
    text
}

fn spawn_roboport_slot_row(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    inventory_panel: InventoryPanel,
    slot_count: usize,
) {
    if slot_count == 0 {
        return;
    }
    panel.spawn((
        Text::new(label.to_string()),
        TextFont::from_font_size(11.0),
        TextColor(Color::srgb(0.70, 0.76, 0.72)),
    ));
    panel
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_wrap: FlexWrap::Wrap,
                row_gap: Val::Px(4.0),
                column_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|grid| {
            for slot_index in 0..slot_count {
                spawn_slot_button(grid, inventory_panel, slot_index);
            }
        });
}

fn spawn_heat_temperature_text(panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    panel.spawn((
        Text::new("Temperature: -"),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.96, 0.78, 0.62)),
        HeatTemperatureText,
    ));
}

pub(crate) fn spawn_inserter_panel(root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(220.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Burner Inserter"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Energy: 0 J"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            BurnerEnergyText,
        ));
        spawn_labeled_slot(
            panel,
            "Fuel",
            InventoryPanel::InserterFuel,
            INSERTER_FUEL_SLOT_INDEX,
        );
    });
}

type RoboportTextQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Text, &'static RoboportReadoutText),
    (Without<BurnerEnergyText>, Without<HeatTemperatureText>),
>;

pub(crate) fn update_machine_indicators(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut energy_texts: Query<&mut Text, With<BurnerEnergyText>>,
    mut temperature_texts: Query<&mut Text, (With<HeatTemperatureText>, Without<BurnerEnergyText>)>,
    mut roboport_texts: RoboportTextQuery,
    mut progress_fills: Query<&mut Node, With<MachineProgressFill>>,
) {
    let sim = sim.read();
    let indicator =
        open_container
            .entity_id
            .and_then(|entity_id| match open_machine_kind(&sim, entity_id)? {
                OpenMachineKind::MiningDrill => {
                    let state =
                        factory_sim::entity_access::mining_drill_state(&sim, entity_id).ok()?;
                    Some((
                        state
                            .energy
                            .burner()
                            .map(|burner| burner.energy_remaining_joules),
                        state.mining_progress_ticks,
                        state.mining_required_ticks,
                    ))
                }
                OpenMachineKind::Furnace => {
                    let state = factory_sim::entity_access::furnace_state(&sim, entity_id).ok()?;
                    Some((
                        state
                            .energy
                            .burner()
                            .map(|burner| burner.energy_remaining_joules),
                        state.crafting_progress_ticks,
                        state.crafting_required_ticks,
                    ))
                }
                OpenMachineKind::Boiler => {
                    let state = factory_sim::entity_access::boiler_state(&sim, entity_id).ok()?;
                    Some((Some(state.energy.energy_remaining_joules), 0, 1))
                }
                OpenMachineKind::Assembler => {
                    let state =
                        factory_sim::entity_access::assembler_state(&sim, entity_id).ok()?;
                    Some((
                        None,
                        state.crafting_progress_ticks,
                        state.crafting_required_ticks,
                    ))
                }
                OpenMachineKind::Inserter => {
                    let energy =
                        factory_sim::entity_access::inserter_energy(&sim, entity_id).ok()?;
                    Some((
                        energy.burner().map(|burner| burner.energy_remaining_joules),
                        0,
                        1,
                    ))
                }
                OpenMachineKind::NuclearReactor => {
                    let state =
                        factory_sim::entity_access::nuclear_reactor_state(&sim, entity_id).ok()?;
                    Some((Some(state.energy.energy_remaining_joules), 0, 1))
                }
                // No burner energy and no crafting progress to report.
                OpenMachineKind::Roboport
                | OpenMachineKind::HeatBuffer
                | OpenMachineKind::Chest
                | OpenMachineKind::Lab
                | OpenMachineKind::Turret
                | OpenMachineKind::Beacon
                | OpenMachineKind::ConstantCombinator
                | OpenMachineKind::ArithmeticCombinator
                | OpenMachineKind::DeciderCombinator
                | OpenMachineKind::Circuit => None,
            });

    for mut text in &mut energy_texts {
        text.0 = indicator
            .and_then(|(energy_remaining_joules, _, _)| energy_remaining_joules)
            .map(|energy_remaining_joules| {
                format!(
                    "Energy: {} J",
                    energy_remaining_joules.max(0.0).round() as u64
                )
            })
            .unwrap_or_else(|| "Energy: 0 J".to_string());
    }

    let heat_status = open_container
        .entity_id
        .and_then(|entity_id| sim.entity_heat_status(entity_id));
    for mut text in &mut temperature_texts {
        text.0 = match heat_status {
            Some(status) => format!(
                "Temperature: {:.1} °C",
                status.temperature_millidegrees as f64 / 1_000.0
            ),
            None => "Temperature: -".to_string(),
        };
    }

    let roboport_status = open_container
        .entity_id
        .and_then(|entity_id| sim.entity_roboport_status(entity_id));
    for (mut text, readout) in &mut roboport_texts {
        text.0 = roboport_readout_text(readout.0, roboport_status, Some(&sim));
    }

    for mut node in &mut progress_fills {
        let progress = indicator
            .map(|(_, progress_ticks, required_ticks)| {
                progress_ticks as f32 / required_ticks.max(1) as f32
            })
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        node.width = Val::Px(MACHINE_BAR_WIDTH * progress);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_guidance_explains_common_blockers_and_resolution() {
        assert_eq!(
            format_machine_guidance(MachineStatus::Working),
            "Working — machine is operating normally."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::Idle),
            "Idle — give this machine work to do."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoFuel),
            "Needs fuel — add a burnable item to the Fuel slot."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoPower),
            "No power — connect the machine to a powered electric network."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::OutputFull),
            "Output blocked — clear the output or connect space for products."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoRecipe),
            "Missing recipe — select a recipe above to begin crafting."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoResearch),
            "No research — select research or unlock the required technology."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoInput),
            "Missing input — add the required ingredients or resources."
        );
        assert_eq!(
            format_machine_guidance(MachineStatus::NoFluid),
            "Missing fluid — connect a pipe carrying the required fluid."
        );
    }
}
