use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::ui::production_stats::components::{
    DiagnosticLine, DiagnosticLinesRoot, DiagnosticSection, PowerGraphBar, PowerGraphEmpty,
    PowerGraphRoot, PowerLine, PowerLinesRoot, ProductionStatsBody, ProductionStatsRocketsText,
    ProductionStatsSnapshot, ProductionStatsTabButton, StatEmptyLabel, StatRow, StatRowsRoot,
    StatSection,
};
use crate::ui::production_stats::snapshot::production_stats_snapshot;
use crate::ui::production_stats::view::{
    graph_height, production_stats_root, spawn_diagnostic_line, spawn_power_graph_point,
    spawn_power_line, spawn_production_stats_body, spawn_production_stats_contents, spawn_stat_row,
};
use crate::ui::resources::{ProductionStatsWindowState, StatsTab};
use crate::ui::window_sync::{
    WindowRootQuery, WindowSyncInput, replace_children_if_different, retained_display,
    sync_retained_window,
};

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
type StatEmptyLabelQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static StatEmptyLabel,
        &'static mut Node,
        &'static mut Visibility,
    ),
    (Without<PowerGraphEmpty>, Without<PowerGraphBar>),
>;
type PowerGraphEmptyQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Node, &'static mut Visibility),
    (
        With<PowerGraphEmpty>,
        Without<PowerGraphBar>,
        Without<StatEmptyLabel>,
    ),
>;
type PowerGraphBarQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static PowerGraphBar, &'static mut Node),
    (Without<PowerGraphEmpty>, Without<StatEmptyLabel>),
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
    row_roots: Query<'w, 's, (Entity, &'static StatRowsRoot, &'static Children)>,
    empty_labels: StatEmptyLabelQuery<'w, 's>,
    rows: Query<'w, 's, (Entity, &'static StatRow, &'static Children)>,
    power_line_roots: Query<'w, 's, (Entity, &'static Children), With<PowerLinesRoot>>,
    power_lines: Query<'w, 's, (Entity, &'static PowerLine)>,
    graph_roots: Query<'w, 's, (Entity, &'static Children), With<PowerGraphRoot>>,
    graph_empty: PowerGraphEmptyQuery<'w, 's>,
    graph_bars: PowerGraphBarQuery<'w, 's>,
    diagnostic_roots: Query<'w, 's, (Entity, &'static DiagnosticLinesRoot, &'static Children)>,
    diagnostic_lines: Query<'w, 's, (Entity, &'static DiagnosticLine)>,
}

pub(crate) fn sync_production_stats_window(
    mut commands: Commands,
    sim: Res<SimResource>,
    state: Res<ProductionStatsWindowState>,
    mut refresh: ResMut<ProductionStatsRefresh>,
    mut roots: WindowRootQuery<ProductionStatsSnapshot>,
    mut nodes: ProductionStatsNodes,
) {
    if !state.open {
        for (entity, _, _) in &mut roots {
            commands.entity(entity).despawn();
        }
        return;
    }

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
    refresh.last_key = Some(key);

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
        commands.entity(entity).insert(Text::new(format!(
            "Rockets launched: {}",
            next.rockets_launched
        )));
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
    let Some((root, current_children)) = nodes
        .row_roots
        .iter()
        .find_map(|(entity, marker, children)| (marker.0 == section).then_some((entity, children)))
    else {
        return;
    };
    let empty =
        nodes
            .empty_labels
            .iter_mut()
            .find_map(|(entity, marker, mut node, mut visibility)| {
                (marker.0 == section).then(|| {
                    let visible = next.is_empty();
                    node.display = retained_display(visible);
                    *visibility = if visible {
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
            .find(|(_, marker, _)| marker.section == section && marker.key == row.key)
        {
            for (child, value) in children
                .iter()
                .zip([&row.item_name, &row.per_minute, &row.total])
            {
                commands.entity(child).insert(Text::new(value.clone()));
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_stat_row(parent, row, section));
            });
        }
    }
    for (entity, marker, _) in &nodes.rows {
        if marker.section == section && !next.iter().any(|row| row.key == marker.key) {
            commands.entity(entity).despawn();
        }
    }
    replace_children_if_different(commands, root, current_children, &ordered);
}

fn reconcile_power_lines(
    commands: &mut Commands,
    next: &[String],
    nodes: &mut ProductionStatsNodes,
) {
    let Some((root, current_children)) = nodes.power_line_roots.iter().next() else {
        return;
    };
    let mut ordered = Vec::with_capacity(next.len());
    for (index, value) in next.iter().enumerate() {
        if let Some((entity, _)) = nodes
            .power_lines
            .iter()
            .find(|(_, marker)| marker.0 == index)
        {
            commands.entity(entity).insert(Text::new(value.clone()));
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
    replace_children_if_different(commands, root, current_children, &ordered);
}

fn reconcile_graph(
    commands: &mut Commands,
    next: &[super::PowerGraphPoint],
    nodes: &mut ProductionStatsNodes,
) {
    let Some((root, current_children)) = nodes.graph_roots.iter().next() else {
        return;
    };
    let max_watts = next
        .iter()
        .flat_map(|point| [point.production_watts, point.consumption_watts])
        .max()
        .unwrap_or(1)
        .max(1);
    let mut ordered = Vec::with_capacity(next.len() * 2 + 1);
    if let Some((empty, mut node, mut visibility)) = nodes.graph_empty.iter_mut().next() {
        let visible = next.is_empty();
        node.display = retained_display(visible);
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        ordered.push(empty);
    }
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
            for existing in existing.into_iter().flatten() {
                commands.entity(existing).despawn();
            }
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
    replace_children_if_different(commands, root, current_children, &ordered);
}

fn reconcile_diagnostics(
    commands: &mut Commands,
    section: DiagnosticSection,
    lines: &[String],
    nodes: &mut ProductionStatsNodes,
) {
    let Some((root, current_children)) = nodes
        .diagnostic_roots
        .iter()
        .find_map(|(entity, marker, children)| (marker.0 == section).then_some((entity, children)))
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
            commands.entity(entity).insert(Text::new(value.clone()));
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
    replace_children_if_different(commands, root, current_children, &ordered);
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::ItemId;

    use super::super::components::{ItemStatDisplayRow, StatRowKey};

    #[derive(Resource, Default)]
    struct GraphFixture(Vec<super::super::PowerGraphPoint>);

    #[derive(Resource)]
    struct StatFixture(Vec<ItemStatDisplayRow>);

    fn reconcile_graph_fixture(
        mut commands: Commands,
        fixture: Res<GraphFixture>,
        mut nodes: ProductionStatsNodes,
    ) {
        reconcile_graph(&mut commands, &fixture.0, &mut nodes);
    }

    fn reconcile_stat_fixture(
        mut commands: Commands,
        fixture: Res<StatFixture>,
        mut nodes: ProductionStatsNodes,
    ) {
        reconcile_stat_rows(&mut commands, StatSection::Items, &fixture.0, &mut nodes);
    }

    #[test]
    fn stat_rows_use_ids_instead_of_display_names_for_identity() {
        let first = stat_fixture_row(ItemId::new(1), "Shared display name");
        let mut app = App::new();
        app.insert_resource(StatFixture(vec![first.clone()]))
            .add_systems(Update, reconcile_stat_fixture);
        let root = app
            .world_mut()
            .spawn((Node::default(), StatRowsRoot(StatSection::Items)))
            .with_children(|parent| {
                parent.spawn((
                    Node::default(),
                    Text::new("<none>"),
                    Visibility::Hidden,
                    StatEmptyLabel(StatSection::Items),
                ));
                parent
                    .spawn((
                        Node::default(),
                        StatRow {
                            section: StatSection::Items,
                            key: first.key,
                        },
                    ))
                    .with_children(|row| {
                        row.spawn(Text::new(first.item_name.clone()));
                        row.spawn(Text::new(first.per_minute.clone()));
                        row.spawn(Text::new(first.total.clone()));
                    });
            })
            .id();
        app.update();
        let original = stat_row_entity(&mut app, first.key);

        let second = stat_fixture_row(ItemId::new(2), "Shared display name");
        app.world_mut().resource_mut::<StatFixture>().0 = vec![second.clone()];
        app.update();

        let replacement = stat_row_entity(&mut app, second.key);
        assert_ne!(replacement, original);
        assert!(app.world().get_entity(original).is_err());
        assert_eq!(children_of(&app, root).len(), 2);
    }

    #[test]
    fn graph_empty_label_survives_empty_populated_empty_transitions() {
        let mut app = App::new();
        app.init_resource::<GraphFixture>()
            .add_systems(Update, reconcile_graph_fixture);
        let root = app
            .world_mut()
            .spawn((Node::default(), PowerGraphRoot))
            .with_child((
                Node::default(),
                Text::new("<no samples>"),
                Visibility::Inherited,
                PowerGraphEmpty,
            ))
            .id();
        app.update();
        let empty = graph_empty_entity(&mut app);

        app.world_mut().resource_mut::<GraphFixture>().0 = vec![super::super::PowerGraphPoint {
            production_watts: 100,
            consumption_watts: 50,
        }];
        app.update();

        assert_eq!(graph_bar_count(&mut app), 2);
        assert_eq!(children_of(&app, root).len(), 3);
        assert_eq!(
            app.world().entity(empty).get::<Node>().unwrap().display,
            Display::None
        );

        app.world_mut().resource_mut::<GraphFixture>().0.clear();
        app.update();

        assert_eq!(graph_bar_count(&mut app), 0);
        assert_eq!(children_of(&app, root), vec![empty]);
        assert_eq!(
            app.world().entity(empty).get::<Node>().unwrap().display,
            Display::Flex
        );
        assert_eq!(
            *app.world().entity(empty).get::<Visibility>().unwrap(),
            Visibility::Inherited
        );
    }

    fn graph_empty_entity(app: &mut App) -> Entity {
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<PowerGraphEmpty>>();
        query.single(world).unwrap()
    }

    fn stat_fixture_row(item_id: ItemId, item_name: &str) -> ItemStatDisplayRow {
        ItemStatDisplayRow {
            key: StatRowKey::Item(item_id),
            item_name: item_name.to_string(),
            per_minute: "1/min".to_string(),
            total: "1".to_string(),
        }
    }

    fn stat_row_entity(app: &mut App, key: StatRowKey) -> Entity {
        let world = app.world_mut();
        let mut query = world.query::<(Entity, &StatRow)>();
        query
            .iter(world)
            .find_map(|(entity, marker)| (marker.key == key).then_some(entity))
            .expect("stat row should exist")
    }

    fn graph_bar_count(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<PowerGraphBar>>();
        query.iter(world).count()
    }

    fn children_of(app: &App, root: Entity) -> Vec<Entity> {
        app.world()
            .entity(root)
            .get::<Children>()
            .unwrap()
            .iter()
            .collect()
    }
}
