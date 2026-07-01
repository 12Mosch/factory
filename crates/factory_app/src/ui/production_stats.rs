use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::{
    FluidStatisticsRow, ItemStatisticsRow, MachineStatus, PowerNetworkSnapshot,
    PowerStatisticsSample, PowerSummary, Simulation,
};

use crate::resources::{ProductionStatsWindowState, SimResource, StatsTab};
use crate::ui::debug_overlay::format_watts;
use crate::ui::formatting::{format_fluid_display_name, format_item_display_name};

const POWER_GRAPH_POINT_COUNT: usize = 40;

#[derive(Component)]
pub(crate) struct ProductionStatsRoot {
    snapshot: ProductionStatsSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProductionStatsSnapshot {
    selected_tab: StatsTab,
    item_rows: Vec<ItemStatDisplayRow>,
    fluid_rows: Vec<ItemStatDisplayRow>,
    power_lines: Vec<String>,
    power_graph: Vec<PowerGraphPoint>,
    diagnostic_lines: Vec<String>,
    bottleneck_lines: Vec<String>,
}

#[derive(Component)]
pub struct ProductionStatsTabButton {
    tab: StatsTab,
}

type StatsTabInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ProductionStatsTabButton),
    (Changed<Interaction>, With<Button>),
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemStatDisplayRow {
    pub item_name: String,
    pub per_minute: String,
    pub total: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerGraphPoint {
    pub production_watts: u64,
    pub consumption_watts: u64,
}

pub(crate) fn handle_production_stats_buttons(
    mut buttons: StatsTabInteractionQuery,
    mut state: ResMut<ProductionStatsWindowState>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction == Interaction::Pressed {
            state.selected_tab = button.tab;
        }
    }
}

pub(crate) fn sync_production_stats_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    state: Res<ProductionStatsWindowState>,
    mut roots: Query<(Entity, &mut ProductionStatsRoot, Option<&Children>)>,
) {
    if !state.open {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        let snapshot = production_stats_snapshot(&sim.sim, state.selected_tab);
        spawn_production_stats_window(&mut commands, snapshot);
        return;
    };
    for (duplicate, _, _) in roots_iter {
        commands.entity(duplicate).despawn();
    }
    if !sim.is_changed() && !state.is_changed() {
        return;
    }
    let snapshot = production_stats_snapshot(&sim.sim, state.selected_tab);
    if root.snapshot == snapshot {
        return;
    }
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    root.snapshot = snapshot.clone();
    commands
        .entity(root_entity)
        .with_children(|root| spawn_production_stats_contents(root, &snapshot));
}

fn spawn_production_stats_window(commands: &mut Commands, snapshot: ProductionStatsSnapshot) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(470.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.028, 0.030, 0.032, 0.97)),
            GlobalZIndex(2300),
            ProductionStatsRoot {
                snapshot: snapshot.clone(),
            },
        ))
        .with_children(|root| spawn_production_stats_contents(root, &snapshot));
}

fn production_stats_snapshot(sim: &Simulation, selected_tab: StatsTab) -> ProductionStatsSnapshot {
    match selected_tab {
        StatsTab::Production => ProductionStatsSnapshot {
            selected_tab,
            item_rows: production_rows(sim),
            fluid_rows: fluid_production_rows(sim),
            power_lines: Vec::new(),
            power_graph: Vec::new(),
            diagnostic_lines: Vec::new(),
            bottleneck_lines: Vec::new(),
        },
        StatsTab::Consumption => ProductionStatsSnapshot {
            selected_tab,
            item_rows: consumption_rows(sim),
            fluid_rows: fluid_consumption_rows(sim),
            power_lines: Vec::new(),
            power_graph: Vec::new(),
            diagnostic_lines: Vec::new(),
            bottleneck_lines: Vec::new(),
        },
        StatsTab::Power => ProductionStatsSnapshot {
            selected_tab,
            item_rows: Vec::new(),
            fluid_rows: Vec::new(),
            power_lines: power_summary_lines(sim.power_summary(), sim.power_networks()),
            power_graph: power_graph_points(
                &sim.power_statistics().samples,
                POWER_GRAPH_POINT_COUNT,
            ),
            diagnostic_lines: Vec::new(),
            bottleneck_lines: Vec::new(),
        },
        StatsTab::Diagnostics => ProductionStatsSnapshot {
            selected_tab,
            item_rows: Vec::new(),
            fluid_rows: Vec::new(),
            power_lines: Vec::new(),
            power_graph: Vec::new(),
            diagnostic_lines: diagnostic_lines(sim),
            bottleneck_lines: bottleneck_lines(sim),
        },
    }
}

fn spawn_production_stats_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &ProductionStatsSnapshot,
) {
    spawn_tabs(root, snapshot.selected_tab);
    match snapshot.selected_tab {
        StatsTab::Production => {
            spawn_item_rows(root, &snapshot.item_rows, "Items", "/min", "Total");
            spawn_item_rows(root, &snapshot.fluid_rows, "Fluids", "/min", "Total");
        }
        StatsTab::Consumption => {
            spawn_item_rows(root, &snapshot.item_rows, "Items", "/min", "Total");
            spawn_item_rows(root, &snapshot.fluid_rows, "Fluids", "/min", "Total");
        }
        StatsTab::Power => spawn_power_rows(root, &snapshot.power_lines, &snapshot.power_graph),
        StatsTab::Diagnostics => {
            spawn_diagnostics_rows(root, &snapshot.diagnostic_lines, &snapshot.bottleneck_lines)
        }
    }
}

pub fn production_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    let mut rows = sim.item_statistics().rows;
    rows.sort_by(|a, b| {
        b.produced_last_minute
            .cmp(&a.produced_last_minute)
            .then_with(|| item_name(sim, a).cmp(&item_name(sim, b)))
    });
    rows.into_iter()
        .filter(|row| row.produced_last_minute > 0 || row.produced_total > 0)
        .map(|row| ItemStatDisplayRow {
            item_name: format_item_display_name(sim.catalog(), row.item_id),
            per_minute: format_per_minute(row.produced_last_minute),
            total: row.produced_total.to_string(),
        })
        .collect()
}

pub fn consumption_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    let mut rows = sim.item_statistics().rows;
    rows.sort_by(|a, b| {
        b.consumed_last_minute
            .cmp(&a.consumed_last_minute)
            .then_with(|| item_name(sim, a).cmp(&item_name(sim, b)))
    });
    rows.into_iter()
        .filter(|row| row.consumed_last_minute > 0 || row.consumed_total > 0)
        .map(|row| ItemStatDisplayRow {
            item_name: format_item_display_name(sim.catalog(), row.item_id),
            per_minute: format_per_minute(row.consumed_last_minute),
            total: row.consumed_total.to_string(),
        })
        .collect()
}

pub fn fluid_production_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    let mut rows = sim.fluid_statistics().rows;
    rows.sort_by(|a, b| {
        b.produced_last_minute
            .cmp(&a.produced_last_minute)
            .then_with(|| fluid_name(sim, a).cmp(&fluid_name(sim, b)))
    });
    rows.into_iter()
        .filter(|row| row.produced_last_minute > 0 || row.produced_total > 0)
        .map(|row| ItemStatDisplayRow {
            item_name: format_fluid_display_name(sim.catalog(), row.fluid_id),
            per_minute: format_fluid_per_minute(row.produced_last_minute),
            total: format_fluid_amount(row.produced_total),
        })
        .collect()
}

pub fn fluid_consumption_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    let mut rows = sim.fluid_statistics().rows;
    rows.sort_by(|a, b| {
        b.consumed_last_minute
            .cmp(&a.consumed_last_minute)
            .then_with(|| fluid_name(sim, a).cmp(&fluid_name(sim, b)))
    });
    rows.into_iter()
        .filter(|row| row.consumed_last_minute > 0 || row.consumed_total > 0)
        .map(|row| ItemStatDisplayRow {
            item_name: format_fluid_display_name(sim.catalog(), row.fluid_id),
            per_minute: format_fluid_per_minute(row.consumed_last_minute),
            total: format_fluid_amount(row.consumed_total),
        })
        .collect()
}

pub fn power_summary_lines(
    summary: PowerSummary,
    networks: &[PowerNetworkSnapshot],
) -> Vec<String> {
    let mut lines = vec![
        format!("Production: {}", format_watts(summary.production_watts)),
        format!(
            "Available: {}",
            format_watts(summary.available_production_watts)
        ),
        format!("Consumption: {}", format_watts(summary.consumption_watts)),
        format!(
            "Satisfaction: {:.1}%",
            f64::from(summary.satisfaction_permyriad) / 100.0
        ),
        format!("Networks: {}", summary.network_count),
    ];
    lines.extend(networks.iter().map(|network| {
        format!(
            "Network {}: poles {}, producers {}, consumers {}, prod {}, avail {}, cons {}, sat {:.1}%",
            network.network_id,
            network.pole_count,
            network.producer_count,
            network.consumer_count,
            format_watts(network.production_watts),
            format_watts(network.available_production_watts),
            format_watts(network.consumption_watts),
            f64::from(network.satisfaction_permyriad) / 100.0
        )
    }));
    lines
}

pub fn power_graph_points(
    samples: &[PowerStatisticsSample],
    max_points: usize,
) -> Vec<PowerGraphPoint> {
    if samples.is_empty() || max_points == 0 {
        return Vec::new();
    }
    if samples.len() <= max_points {
        return samples
            .iter()
            .map(|sample| PowerGraphPoint {
                production_watts: sample.production_watts,
                consumption_watts: sample.consumption_watts,
            })
            .collect();
    }

    let chunk_size = samples.len().div_ceil(max_points);
    samples
        .chunks(chunk_size)
        .map(|chunk| PowerGraphPoint {
            production_watts: chunk
                .iter()
                .map(|sample| sample.production_watts)
                .max()
                .unwrap_or(0),
            consumption_watts: chunk
                .iter()
                .map(|sample| sample.consumption_watts)
                .max()
                .unwrap_or(0),
        })
        .collect()
}

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

fn spawn_tabs(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, selected: StatsTab) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|tabs| {
            for (tab, label) in [
                (StatsTab::Production, "Production"),
                (StatsTab::Consumption, "Consumption"),
                (StatsTab::Power, "Power"),
                (StatsTab::Diagnostics, "Diagnostics"),
            ] {
                tabs.spawn((
                    Button,
                    Node {
                        height: Val::Px(32.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::horizontal(Val::Px(12.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(if tab == selected {
                        Color::srgba(0.22, 0.27, 0.24, 0.98)
                    } else {
                        Color::srgba(0.10, 0.11, 0.11, 0.98)
                    }),
                    BorderColor::all(Color::srgba(0.38, 0.42, 0.36, 0.85)),
                    ProductionStatsTabButton { tab },
                ))
                .with_child((
                    Text::new(label),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                ));
            }
        });
}

fn spawn_item_rows(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    rows: &[ItemStatDisplayRow],
    title: &str,
    rate_header: &str,
    total_header: &str,
) {
    parent.spawn((
        Text::new(title),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    spawn_row(parent, "Item", rate_header, total_header, true);
    if rows.is_empty() {
        parent.spawn((
            Text::new("<none>"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.62, 0.64, 0.60)),
        ));
        return;
    }
    for row in rows {
        spawn_row(parent, &row.item_name, &row.per_minute, &row.total, false);
    }
}

fn spawn_power_rows(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    lines: &[String],
    graph: &[PowerGraphPoint],
) {
    parent.spawn((
        Text::new("Power"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    spawn_power_graph(parent, graph);
    for line in lines {
        parent.spawn((
            Text::new(line.clone()),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.84, 0.86, 0.80)),
        ));
    }
}

fn spawn_power_graph(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    graph: &[PowerGraphPoint],
) {
    if graph.is_empty() {
        parent.spawn((
            Text::new("<no samples>"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.62, 0.64, 0.60)),
        ));
        return;
    }

    let max_watts = graph
        .iter()
        .flat_map(|point| [point.production_watts, point.consumption_watts])
        .max()
        .unwrap_or(1)
        .max(1);
    parent
        .spawn((
            Node {
                height: Val::Px(82.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::End,
                column_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.055, 0.058, 0.060, 0.88)),
        ))
        .with_children(|bars| {
            for point in graph {
                let production_height =
                    ((point.production_watts as f32 / max_watts as f32) * 68.0).max(1.0);
                let consumption_height =
                    ((point.consumption_watts as f32 / max_watts as f32) * 68.0).max(1.0);
                bars.spawn((
                    Node {
                        width: Val::Px(4.0),
                        height: Val::Px(production_height),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.30, 0.72, 0.42)),
                ));
                bars.spawn((
                    Node {
                        width: Val::Px(4.0),
                        height: Val::Px(consumption_height),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.88, 0.58, 0.22)),
                ));
            }
        });
}

fn spawn_diagnostics_rows(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    diagnostic_lines: &[String],
    bottleneck_lines: &[String],
) {
    parent.spawn((
        Text::new("Machine Status"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    if diagnostic_lines.is_empty() {
        parent.spawn((
            Text::new("<none>"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.62, 0.64, 0.60)),
        ));
    } else {
        for line in diagnostic_lines {
            parent.spawn((
                Text::new(line.clone()),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.84, 0.86, 0.80)),
            ));
        }
    }

    parent.spawn((
        Text::new("Bottlenecks"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    if bottleneck_lines.is_empty() {
        parent.spawn((
            Text::new("<none>"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.62, 0.64, 0.60)),
        ));
    } else {
        for line in bottleneck_lines {
            parent.spawn((
                Text::new(line.clone()),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.94, 0.78, 0.48)),
            ));
        }
    }
}

fn spawn_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    left: &str,
    middle: &str,
    right: &str,
    header: bool,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                min_height: Val::Px(if header { 26.0 } else { 24.0 }),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(if header {
                Color::srgba(0.12, 0.13, 0.13, 0.96)
            } else {
                Color::srgba(0.055, 0.058, 0.060, 0.88)
            }),
        ))
        .with_children(|row| {
            for (text, width) in [(left, 210.0), (middle, 90.0), (right, 90.0)] {
                row.spawn((
                    Node {
                        width: Val::Px(width),
                        ..default()
                    },
                    Text::new(text.to_string()),
                    TextFont::from_font_size(if header { 11.0 } else { 12.0 }),
                    TextColor(Color::srgb(0.88, 0.90, 0.84)),
                ));
            }
        });
}

fn item_name(sim: &Simulation, row: &ItemStatisticsRow) -> String {
    format_item_display_name(sim.catalog(), row.item_id)
}

fn fluid_name(sim: &Simulation, row: &FluidStatisticsRow) -> String {
    format_fluid_display_name(sim.catalog(), row.fluid_id)
}

pub fn format_per_minute_u64(value: u64) -> String {
    format!("{value}/min")
}

fn format_per_minute(value: u64) -> String {
    format_per_minute_u64(value)
}

pub fn format_fluid_per_minute(milliunits: u64) -> String {
    format!("{}/min", format_fluid_amount(milliunits))
}

fn format_fluid_amount(milliunits: u64) -> String {
    let whole = milliunits / 1_000;
    let remainder = milliunits % 1_000;
    if remainder == 0 {
        whole.to_string()
    } else {
        let tenths = (remainder / 100).min(9);
        format!("{whole}.{tenths}")
    }
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
        EntityKind::Pipe => "Pipes",
        EntityKind::StorageTank => "Storage tanks",
    }
}
