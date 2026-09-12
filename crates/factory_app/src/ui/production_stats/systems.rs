use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::ui::production_stats::components::{
    DiagnosticLine, DiagnosticLinesRoot, DiagnosticSection, PowerGraphBar, PowerGraphRoot,
    PowerLine, PowerLinesRoot, ProductionStatsBody, ProductionStatsRocketsText,
    ProductionStatsSnapshot, ProductionStatsTabButton, StatEmptyLabel, StatRow, StatRowsRoot,
    StatSection,
};
use crate::ui::production_stats::snapshot::production_stats_snapshot;
use crate::ui::production_stats::view::{
    graph_height, production_stats_root, spawn_diagnostic_line, spawn_power_graph_point,
    spawn_power_line, spawn_production_stats_body, spawn_production_stats_contents, spawn_stat_row,
};
use crate::ui::resources::{ProductionStatsWindowState, StatsTab};
use crate::ui::window_sync::{WindowRootQuery, WindowSyncInput, sync_retained_window};

/// Informational statistics do not need to be reformatted at simulation tick
/// rate. Four refreshes per simulated second keeps the panel responsive while
/// collapsing bursts of fixed ticks into one UI update.
const STATS_REFRESH_TICKS: u64 = 15;

#[derive(Resource, Default)]
pub(crate) struct ProductionStatsRefresh {
    last_key: Option<ProductionStatsRefreshKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProductionStatsRefreshKey {
    replacement_revision: u64,
    tick_bucket: u64,
    selected_tab: StatsTab,
    diagnostic_revision: u64,
}

type StatsTabInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static ProductionStatsTabButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_production_stats_buttons(
    mut buttons: StatsTabInteractionQuery,
    mut state: ResMut<ProductionStatsWindowState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            state.selected_tab = button.tab;
        }
    }
}

#[derive(SystemParam)]
pub(crate) struct ProductionStatsNodes<'w, 's> {
    bodies: Query<'w, 's, Entity, With<ProductionStatsBody>>,
    rocket_labels: Query<'w, 's, Entity, With<ProductionStatsRocketsText>>,
    tabs: Query<
        'w,
        's,
        (
            &'static ProductionStatsTabButton,
            &'static mut BackgroundColor,
        ),
    >,
    row_roots: Query<'w, 's, (Entity, &'static StatRowsRoot)>,
    empty_labels: Query<'w, 's, (Entity, &'static StatEmptyLabel, &'static mut Visibility)>,
    rows: Query<'w, 's, (Entity, &'static StatRow, &'static Children)>,
    power_line_roots: Query<'w, 's, Entity, With<PowerLinesRoot>>,
    power_lines: Query<'w, 's, (Entity, &'static PowerLine)>,
    graph_roots: Query<'w, 's, Entity, With<PowerGraphRoot>>,
    graph_bars: Query<'w, 's, (Entity, &'static PowerGraphBar, &'static mut Node)>,
    diagnostic_roots: Query<'w, 's, (Entity, &'static DiagnosticLinesRoot)>,
    diagnostic_lines: Query<'w, 's, (Entity, &'static DiagnosticLine)>,
    texts: Query<'w, 's, &'static mut Text>,
}

pub(crate) fn sync_production_stats_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    state: Res<ProductionStatsWindowState>,
    mut refresh: ResMut<ProductionStatsRefresh>,
    mut roots: WindowRootQuery<ProductionStatsSnapshot>,
    mut nodes: ProductionStatsNodes,
) {
    let simulation = sim.read();
    let key = ProductionStatsRefreshKey {
        replacement_revision: sim.replacement_revision(),
        tick_bucket: simulation.tick_count() / STATS_REFRESH_TICKS,
        selected_tab: state.selected_tab,
        diagnostic_revision: if state.selected_tab == StatsTab::Diagnostics {
            simulation.production_status_revision()
        } else {
            0
        },
    };
    let inputs_changed = refresh.last_key != Some(key) || state.is_changed();
    if state.open {
        refresh.last_key = Some(key);
    }

    sync_retained_window(
        &mut commands,
        &mut roots,
        WindowSyncInput {
            open: state.open,
            changed: inputs_changed,
        },
        || production_stats_snapshot(&simulation, state.selected_tab),
        production_stats_root,
        spawn_production_stats_contents,
        |commands, root, previous, next| {
            update_production_stats(commands, root, previous, next, &mut nodes);
        },
    );
}

fn update_production_stats(
    commands: &mut Commands,
    root: Entity,
    previous: &ProductionStatsSnapshot,
    next: &ProductionStatsSnapshot,
    nodes: &mut ProductionStatsNodes,
) {
    for entity in &nodes.rocket_labels {
        if let Ok(mut text) = nodes.texts.get_mut(entity) {
            text.0 = format!("Rockets launched: {}", next.rockets_launched);
        }
    }
    for (button, mut background) in &mut nodes.tabs {
        background.0 = if button.tab == next.selected_tab {
            Color::srgba(0.22, 0.27, 0.24, 0.98)
        } else {
            Color::srgba(0.10, 0.11, 0.11, 0.98)
        };
    }

    if previous.selected_tab != next.selected_tab {
        for body in &nodes.bodies {
            commands.entity(body).despawn();
        }
        commands.entity(root).with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                ProductionStatsBody,
            ))
            .with_children(|body| spawn_production_stats_body(body, next));
        });
        return;
    }

    match next.selected_tab {
        StatsTab::Production | StatsTab::Consumption => {
            reconcile_stat_rows(commands, StatSection::Items, &next.item_rows, nodes);
            reconcile_stat_rows(commands, StatSection::Fluids, &next.fluid_rows, nodes);
        }
        StatsTab::Power => {
            reconcile_power_lines(commands, &next.power_lines, nodes);
            reconcile_graph(commands, &next.power_graph, nodes);
        }
        StatsTab::Diagnostics => {
            reconcile_diagnostics(
                commands,
                DiagnosticSection::Status,
                &next.diagnostic_lines,
                nodes,
            );
            reconcile_diagnostics(
                commands,
                DiagnosticSection::Bottleneck,
                &next.bottleneck_lines,
                nodes,
            );
        }
    }
}

fn reconcile_stat_rows(
    commands: &mut Commands,
    section: StatSection,
    next: &[super::ItemStatDisplayRow],
    nodes: &mut ProductionStatsNodes,
) {
    let Some(root) = nodes
        .row_roots
        .iter()
        .find_map(|(entity, marker)| (marker.0 == section).then_some(entity))
    else {
        return;
    };
    let empty = nodes
        .empty_labels
        .iter_mut()
        .find_map(|(entity, marker, mut visibility)| {
            (marker.0 == section).then(|| {
                *visibility = if next.is_empty() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                entity
            })
        });

    let mut ordered = Vec::with_capacity(next.len() + usize::from(empty.is_some()));
    if let Some(empty) = empty {
        ordered.push(empty);
    }
    for row in next {
        if let Some((entity, _, children)) = nodes
            .rows
            .iter()
            .find(|(_, marker, _)| marker.section == section && marker.key == row.item_name)
        {
            for (child, value) in children
                .iter()
                .zip([&row.item_name, &row.per_minute, &row.total])
            {
                if let Ok(mut text) = nodes.texts.get_mut(child)
                    && text.0 != *value
                {
                    text.0.clone_from(value);
                }
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_stat_row(parent, row, section));
            });
        }
    }
    for (entity, marker, _) in &nodes.rows {
        if marker.section == section && !next.iter().any(|row| row.item_name == marker.key) {
            commands.entity(entity).despawn();
        }
    }
    commands.entity(root).replace_children(&ordered);
}

fn reconcile_power_lines(
    commands: &mut Commands,
    next: &[String],
    nodes: &mut ProductionStatsNodes,
) {
    let Some(root) = nodes.power_line_roots.iter().next() else {
        return;
    };
    let mut ordered = Vec::with_capacity(next.len());
    for (index, value) in next.iter().enumerate() {
        if let Some((entity, _)) = nodes
            .power_lines
            .iter()
            .find(|(_, marker)| marker.0 == index)
        {
            if let Ok(mut text) = nodes.texts.get_mut(entity)
                && text.0 != *value
            {
                text.0.clone_from(value);
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_power_line(parent, index, value));
            });
        }
    }
    for (entity, marker) in &nodes.power_lines {
        if marker.0 >= next.len() {
            commands.entity(entity).despawn();
        }
    }
    commands.entity(root).replace_children(&ordered);
}

fn reconcile_graph(
    commands: &mut Commands,
    next: &[super::PowerGraphPoint],
    nodes: &mut ProductionStatsNodes,
) {
    let Some(root) = nodes.graph_roots.iter().next() else {
        return;
    };
    let max_watts = next
        .iter()
        .flat_map(|point| [point.production_watts, point.consumption_watts])
        .max()
        .unwrap_or(1)
        .max(1);
    let mut ordered = Vec::with_capacity(next.len() * 2);
    for (index, point) in next.iter().enumerate() {
        let mut existing = [None, None];
        for (entity, marker, mut node) in &mut nodes.graph_bars {
            if marker.index != index {
                continue;
            }
            let watts = if marker.production {
                point.production_watts
            } else {
                point.consumption_watts
            };
            node.height = Val::Px(graph_height(watts, max_watts));
            existing[usize::from(!marker.production)] = Some(entity);
        }
        if let [Some(production), Some(consumption)] = existing {
            ordered.extend([production, consumption]);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.extend(spawn_power_graph_point(parent, index, *point, max_watts));
            });
        }
    }
    for (entity, marker, _) in &nodes.graph_bars {
        if marker.index >= next.len() {
            commands.entity(entity).despawn();
        }
    }
    commands.entity(root).replace_children(&ordered);
}

fn reconcile_diagnostics(
    commands: &mut Commands,
    section: DiagnosticSection,
    lines: &[String],
    nodes: &mut ProductionStatsNodes,
) {
    let Some(root) = nodes
        .diagnostic_roots
        .iter()
        .find_map(|(entity, marker)| (marker.0 == section).then_some(entity))
    else {
        return;
    };
    let fallback = String::from("<none>");
    let lines = if lines.is_empty() {
        std::slice::from_ref(&fallback)
    } else {
        lines
    };
    let mut ordered = Vec::with_capacity(lines.len());
    for (index, value) in lines.iter().enumerate() {
        if let Some((entity, _)) = nodes
            .diagnostic_lines
            .iter()
            .find(|(_, marker)| marker.section == section && marker.index == index)
        {
            if let Ok(mut text) = nodes.texts.get_mut(entity)
                && text.0 != *value
            {
                text.0.clone_from(value);
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_diagnostic_line(parent, section, index, value));
            });
        }
    }
    for (entity, marker) in &nodes.diagnostic_lines {
        if marker.section == section && marker.index >= lines.len() {
            commands.entity(entity).despawn();
        }
    }
    commands.entity(root).replace_children(&ordered);
}
