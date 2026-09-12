use bevy::prelude::*;

use crate::ui::production_stats::components::{
    DiagnosticLine, DiagnosticLinesRoot, DiagnosticSection, PowerGraphBar, PowerGraphEmpty,
    PowerGraphRoot, PowerLine, PowerLinesRoot, ProductionStatsBody, ProductionStatsRocketsText,
    ProductionStatsSnapshot, StatEmptyLabel, StatRow, StatRowsRoot, StatSection,
};
use crate::ui::production_stats::{ItemStatDisplayRow, PowerGraphPoint, ProductionStatsTabButton};
use crate::ui::resources::StatsTab;
use crate::ui::window_sync::retained_display;

const POWER_GRAPH_BAR_WIDTH_PX: f32 = 3.0;
const POWER_GRAPH_BAR_GAP_PX: f32 = 1.0;

pub(crate) fn production_stats_root() -> impl Bundle {
    (
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
    )
}

pub(crate) fn spawn_production_stats_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &ProductionStatsSnapshot,
) {
    spawn_tabs(root, snapshot.selected_tab);
    root.spawn((
        Text::new(format!("Rockets launched: {}", snapshot.rockets_launched)),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.92, 0.82, 0.45)),
        ProductionStatsRocketsText,
    ));
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        ProductionStatsBody,
    ))
    .with_children(|body| spawn_production_stats_body(body, snapshot));
}

pub(super) fn spawn_production_stats_body(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &ProductionStatsSnapshot,
) {
    match snapshot.selected_tab {
        StatsTab::Production | StatsTab::Consumption => {
            spawn_item_rows(
                root,
                &snapshot.item_rows,
                "Items",
                "/min",
                "Total",
                StatSection::Items,
            );
            spawn_item_rows(
                root,
                &snapshot.fluid_rows,
                "Fluids",
                "/min",
                "Total",
                StatSection::Fluids,
            );
        }
        StatsTab::Power => spawn_power_rows(root, &snapshot.power_lines, &snapshot.power_graph),
        StatsTab::Diagnostics => {
            spawn_diagnostics_rows(root, &snapshot.diagnostic_lines, &snapshot.bottleneck_lines)
        }
    }
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
    section: StatSection,
) {
    parent.spawn((
        Text::new(title),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    spawn_row(parent, "Item", rate_header, total_header, true, None);
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            StatRowsRoot(section),
        ))
        .with_children(|row_root| {
            row_root.spawn((
                Text::new("<none>"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.62, 0.64, 0.60)),
                StatEmptyLabel(section),
                Node {
                    display: retained_display(rows.is_empty()),
                    ..default()
                },
                if rows.is_empty() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
            for row in rows {
                spawn_stat_row(row_root, row, section);
            }
        });
}

pub(super) fn spawn_stat_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    row: &ItemStatDisplayRow,
    section: StatSection,
) -> Entity {
    spawn_row(
        parent,
        &row.item_name,
        &row.per_minute,
        &row.total,
        false,
        Some(StatRow {
            section,
            key: row.key,
        }),
    )
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
    parent
        .spawn((
            PowerLinesRoot,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|line_root| {
            for (index, line) in lines.iter().enumerate() {
                spawn_power_line(line_root, index, line);
            }
        });
}

pub(super) fn spawn_power_line(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    index: usize,
    line: &str,
) -> Entity {
    parent
        .spawn((
            Text::new(line),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.84, 0.86, 0.80)),
            PowerLine(index),
        ))
        .id()
}

fn spawn_power_graph(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    graph: &[PowerGraphPoint],
) {
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
                column_gap: Val::Px(POWER_GRAPH_BAR_GAP_PX),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.055, 0.058, 0.060, 0.88)),
            PowerGraphRoot,
        ))
        .with_children(|bars| {
            bars.spawn((
                Text::new("<no samples>"),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.62, 0.64, 0.60)),
                Node {
                    display: retained_display(graph.is_empty()),
                    ..default()
                },
                if graph.is_empty() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                PowerGraphEmpty,
            ));
            for (index, point) in graph.iter().enumerate() {
                spawn_power_graph_point(bars, index, *point, max_watts);
            }
        });
}

pub(super) fn spawn_power_graph_point(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    index: usize,
    point: PowerGraphPoint,
    max_watts: u64,
) -> [Entity; 2] {
    let production_height = graph_height(point.production_watts, max_watts);
    let consumption_height = graph_height(point.consumption_watts, max_watts);
    let production = parent
        .spawn((
            Node {
                width: Val::Px(POWER_GRAPH_BAR_WIDTH_PX),
                height: Val::Px(production_height),
                ..default()
            },
            BackgroundColor(Color::srgb(0.30, 0.72, 0.42)),
            PowerGraphBar {
                index,
                production: true,
            },
        ))
        .id();
    let consumption = parent
        .spawn((
            Node {
                width: Val::Px(POWER_GRAPH_BAR_WIDTH_PX),
                height: Val::Px(consumption_height),
                ..default()
            },
            BackgroundColor(Color::srgb(0.88, 0.58, 0.22)),
            PowerGraphBar {
                index,
                production: false,
            },
        ))
        .id();
    [production, consumption]
}

pub(super) fn graph_height(watts: u64, max_watts: u64) -> f32 {
    ((watts as f32 / max_watts.max(1) as f32) * 68.0).max(1.0)
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
    spawn_diagnostic_line_group(parent, diagnostic_lines, DiagnosticSection::Status);

    parent.spawn((
        Text::new("Bottlenecks"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    spawn_diagnostic_line_group(parent, bottleneck_lines, DiagnosticSection::Bottleneck);
}

fn spawn_diagnostic_line_group(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    lines: &[String],
    section: DiagnosticSection,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            DiagnosticLinesRoot(section),
        ))
        .with_children(|line_root| {
            if lines.is_empty() {
                spawn_diagnostic_line(line_root, section, 0, "<none>");
            } else {
                for (index, line) in lines.iter().enumerate() {
                    spawn_diagnostic_line(line_root, section, index, line);
                }
            }
        });
}

pub(super) fn spawn_diagnostic_line(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    section: DiagnosticSection,
    index: usize,
    line: &str,
) -> Entity {
    parent
        .spawn((
            Text::new(line),
            TextFont::from_font_size(12.0),
            TextColor(if section == DiagnosticSection::Bottleneck {
                Color::srgb(0.94, 0.78, 0.48)
            } else {
                Color::srgb(0.84, 0.86, 0.80)
            }),
            DiagnosticLine { section, index },
        ))
        .id()
}

fn spawn_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    left: &str,
    middle: &str,
    right: &str,
    header: bool,
    marker: Option<StatRow>,
) -> Entity {
    let mut entity = parent.spawn((
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
    ));
    if let Some(marker) = marker {
        entity.insert(marker);
    }
    entity
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
        })
        .id()
}
