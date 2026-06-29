use bevy::prelude::*;
use factory_sim::{ItemStatisticsRow, PowerNetworkSnapshot, PowerSummary, Simulation};

use crate::resources::{ProductionStatsWindowState, SimResource, StatsTab};
use crate::ui::debug_overlay::format_watts;
use crate::ui::formatting::format_item_display_name;

#[derive(Component)]
pub(crate) struct ProductionStatsRoot;

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
    roots: Query<Entity, With<ProductionStatsRoot>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if !state.open {
        return;
    }

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
            ProductionStatsRoot,
        ))
        .with_children(|root| {
            spawn_tabs(root, state.selected_tab);
            match state.selected_tab {
                StatsTab::Production => spawn_item_rows(
                    root,
                    &production_rows(&sim.sim),
                    "Production",
                    "/min",
                    "Total",
                ),
                StatsTab::Consumption => spawn_item_rows(
                    root,
                    &consumption_rows(&sim.sim),
                    "Consumption",
                    "/min",
                    "Total",
                ),
                StatsTab::Power => spawn_power_rows(root, &sim.sim),
            }
        });
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

fn spawn_power_rows(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, sim: &Simulation) {
    parent.spawn((
        Text::new("Power"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));
    for line in power_summary_lines(sim.power_summary(), sim.power_networks()) {
        parent.spawn((
            Text::new(line),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.84, 0.86, 0.80)),
        ));
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

fn format_per_minute(value: u64) -> String {
    format!("{value}/min")
}
