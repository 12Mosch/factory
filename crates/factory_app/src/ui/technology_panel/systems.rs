use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::SimCommand;

use crate::audio::SoundEvent;
use crate::build::resources::{BuildMenuState, BuildPlacementState};
use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::{escape_consumed, world_input_blocked};
use crate::input::resources::AppInputState;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::TechnologyWindowState;

use super::components::{
    ActiveResearchText, ResearchQueueText, TechnologyDetailField, TechnologyDetailRoot,
    TechnologyDetailText, TechnologyPanelContentRoot, TechnologyPanelSnapshot,
    TechnologyProgressFill, TechnologyQueueAction, TechnologyQueueButton, TechnologyQueueEmpty,
    TechnologyQueueRoot, TechnologyQueueRow, TechnologyQueueTitle, TechnologySelectButton,
    TechnologyStartQueueButton, TechnologyStatusText,
};
use super::helpers::{
    active_research_text, can_enqueue_for_ui, next_science_cost_text, queue_text,
    start_queue_label, technology_effect_text, technology_progress_text, technology_ui_state,
};
use super::view::{
    research_progress_percent, spawn_technology_detail, spawn_technology_panel_contents,
    spawn_technology_queue_row, technology_panel_root, technology_status_text,
};
use crate::ui::window_sync::{
    WindowRootQuery, WindowSyncInput, replace_children_if_different, retained_display,
    sync_retained_window,
};

type TechnologySelectInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static TechnologySelectButton),
    (Changed<Interaction>, With<Button>),
>;
type TechnologyStartInteractionQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<TechnologyStartQueueButton>,
    ),
>;
type TechnologyQueueInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static TechnologyQueueButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_technology_window_input(
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    build_menu: Res<BuildMenuState>,
    mut window_state: ResMut<TechnologyWindowState>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    if build_menu.open
        || escape_consumed(input_state.as_deref())
        || world_input_blocked(input_state.as_deref())
    {
        return;
    }

    if actions.just_pressed(InputAction::OpenTechnology) {
        window_state.open = !window_state.open;
        if window_state.open {
            build_state.selected = None;
        }
    }

    if window_state.open && actions.just_pressed(InputAction::CancelPause) {
        window_state.open = false;
    }
}

pub(crate) fn ensure_selected_technology(
    mut window_state: ResMut<TechnologyWindowState>,
    sim: Res<SimResource>,
) {
    if !window_state.open {
        return;
    }

    if window_state
        .selected
        .is_some_and(|technology_id| sim.read().catalog().technology(technology_id).is_some())
    {
        return;
    }

    window_state.selected = sim
        .read()
        .active_research()
        .or_else(|| {
            sim.read()
                .catalog()
                .technologies()
                .iter()
                .find(|technology| !sim.read().is_technology_unlocked(technology.id))
                .map(|technology| technology.id)
        })
        .or_else(|| {
            sim.read()
                .catalog()
                .technologies()
                .first()
                .map(|technology| technology.id)
        });
}

pub(crate) fn handle_technology_panel_buttons(
    mut select_buttons: TechnologySelectInteractionQuery,
    mut start_buttons: TechnologyStartInteractionQuery,
    mut queue_buttons: TechnologyQueueInteractionQuery,
    sim: Res<SimResource>,
    mut window_state: ResMut<TechnologyWindowState>,
    mut sounds: MessageWriter<SoundEvent>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if !window_state.open {
        return;
    }

    for (interaction, button) in &mut select_buttons {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            window_state.selected = Some(button.technology_id);
        }
    }

    for interaction in &mut start_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(technology_id) = window_state.selected else {
            continue;
        };
        if !can_enqueue_for_ui(&sim.read(), technology_id) {
            continue;
        }

        commands.write(SimCommandRequest(SimCommand::EnqueueResearch(
            technology_id,
        )));
    }

    for (interaction, button) in &mut queue_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button.action {
            TechnologyQueueAction::Remove => {
                commands.write(SimCommandRequest(SimCommand::RemoveQueuedResearch {
                    index: button.index,
                }));
            }
            TechnologyQueueAction::MoveUp => {
                if button.index > 0 {
                    commands.write(SimCommandRequest(SimCommand::MoveQueuedResearch {
                        from_index: button.index,
                        to_index: button.index - 1,
                    }));
                }
            }
            TechnologyQueueAction::MoveDown => {
                commands.write(SimCommandRequest(SimCommand::MoveQueuedResearch {
                    from_index: button.index,
                    to_index: button.index + 1,
                }));
            }
        }
    }
}

pub(crate) fn sync_technology_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    window_state: Res<TechnologyWindowState>,
    mut roots: WindowRootQuery<TechnologyPanelSnapshot>,
    mut nodes: TechnologyPanelNodes,
) {
    if !window_state.open {
        for (entity, _, _) in &mut roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let simulation = sim.read();
    let snapshot = TechnologyPanelSnapshot {
        replacement_revision: sim.replacement_revision(),
        progress_revision: simulation.research_progress_revision(),
        queue_revision: simulation.research_queue_revision(),
        unlock_revision: simulation.research_unlock_revision(),
        selected: window_state.selected,
    };
    sync_retained_window(
        &mut commands,
        &mut roots,
        WindowSyncInput {
            open: window_state.open,
            changed: true,
        },
        || snapshot,
        technology_panel_root,
        |root, _| spawn_technology_panel_contents(root, &simulation, window_state.selected),
        |commands, root, previous, next| {
            update_technology_panel(commands, root, &simulation, previous, next, &mut nodes);
        },
    );
}

type TechnologyButtonQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static TechnologySelectButton,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (
        With<TechnologySelectButton>,
        Without<TechnologyStartQueueButton>,
    ),
>;
type TechnologyStartButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut BackgroundColor, &'static mut BorderColor),
    (
        With<TechnologyStartQueueButton>,
        Without<TechnologySelectButton>,
    ),
>;
type TechnologyQueueEmptyQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static mut Node, &'static mut Visibility),
    (
        With<TechnologyQueueEmpty>,
        Without<TechnologyQueueButton>,
        Without<TechnologyProgressFill>,
    ),
>;
type TechnologyProgressFillQuery<'w, 's> = Query<
    'w,
    's,
    &'static mut Node,
    (
        With<TechnologyProgressFill>,
        Without<TechnologyQueueButton>,
        Without<TechnologyQueueEmpty>,
    ),
>;
type TechnologyQueueButtonQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut TechnologyQueueButton,
        &'static mut Node,
        &'static mut Visibility,
    ),
    (
        Without<TechnologyQueueEmpty>,
        Without<TechnologyProgressFill>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct TechnologyPanelNodes<'w, 's> {
    content_roots: Query<'w, 's, Entity, With<TechnologyPanelContentRoot>>,
    detail_roots: Query<'w, 's, Entity, With<TechnologyDetailRoot>>,
    active_labels: Query<'w, 's, Entity, With<ActiveResearchText>>,
    queue_labels: Query<'w, 's, Entity, With<ResearchQueueText>>,
    technology_buttons: TechnologyButtonQuery<'w, 's>,
    status_labels: Query<'w, 's, (Entity, &'static TechnologyStatusText)>,
    detail_labels: Query<'w, 's, (Entity, &'static TechnologyDetailText)>,
    progress_fills: TechnologyProgressFillQuery<'w, 's>,
    start_buttons: TechnologyStartButtonQuery<'w, 's>,
    queue_roots: Query<'w, 's, (Entity, &'static Children), With<TechnologyQueueRoot>>,
    queue_titles: Query<'w, 's, Entity, With<TechnologyQueueTitle>>,
    queue_empty: TechnologyQueueEmptyQuery<'w, 's>,
    queue_rows: Query<'w, 's, (Entity, &'static TechnologyQueueRow, &'static Children)>,
    queue_buttons: TechnologyQueueButtonQuery<'w, 's>,
}

fn update_technology_panel(
    commands: &mut Commands,
    root: Entity,
    sim: &factory_sim::Simulation,
    previous: &TechnologyPanelSnapshot,
    next: &TechnologyPanelSnapshot,
    nodes: &mut TechnologyPanelNodes,
) {
    if previous.replacement_revision != next.replacement_revision {
        commands
            .entity(root)
            .despawn_children()
            .with_children(|root| {
                spawn_technology_panel_contents(root, sim, next.selected);
            });
        return;
    }

    let selection_changed = previous.selected != next.selected;
    let progress_changed = previous.progress_revision != next.progress_revision;
    let queue_changed = previous.queue_revision != next.queue_revision;
    let unlock_changed = previous.unlock_revision != next.unlock_revision;

    if progress_changed || queue_changed {
        set_marked_text(commands, &nodes.active_labels, active_research_text(sim));
    }
    if queue_changed {
        set_marked_text(commands, &nodes.queue_labels, queue_text(sim));
    }
    if queue_changed || unlock_changed {
        refresh_technology_list(commands, sim, nodes);
    }
    if selection_changed {
        refresh_selected_borders(previous.selected, next.selected, nodes);
        for detail in &nodes.detail_roots {
            commands.entity(detail).despawn();
        }
        if let Some(content) = nodes.content_roots.iter().next() {
            commands
                .entity(content)
                .with_children(|content| spawn_technology_detail(content, sim, next.selected));
        }
        return;
    }

    if let Some(selected) = next.selected
        && let Some(technology) = sim.catalog().technology(selected)
    {
        if unlock_changed {
            set_detail_text(
                commands,
                &nodes.detail_labels,
                TechnologyDetailField::Level,
                format!(
                    "Current level: {} · {}",
                    sim.technology_level(selected).unwrap_or(0),
                    if technology.level_model.is_repeatable() {
                        "Repeatable"
                    } else {
                        "Finite"
                    }
                ),
            );
            set_detail_text(
                commands,
                &nodes.detail_labels,
                TechnologyDetailField::Cost,
                format!("Cost: {}", next_science_cost_text(sim, technology)),
            );
            set_detail_text(
                commands,
                &nodes.detail_labels,
                TechnologyDetailField::Effects,
                format!("Effects: {}", technology_effect_text(sim, technology)),
            );
        }
        if (progress_changed && sim.active_research() == Some(selected)) || unlock_changed {
            set_detail_text(
                commands,
                &nodes.detail_labels,
                TechnologyDetailField::Progress,
                format!("Progress: {}", technology_progress_text(sim, selected)),
            );
            for mut node in &mut nodes.progress_fills {
                node.width = Val::Percent(research_progress_percent(sim, selected));
            }
        }
        if queue_changed || unlock_changed {
            set_detail_text(
                commands,
                &nodes.detail_labels,
                TechnologyDetailField::Start,
                start_queue_label(sim, selected),
            );
            refresh_start_button(sim, selected, nodes);
        }
        if queue_changed {
            reconcile_research_queue(commands, sim, nodes);
        }
    }
}

fn refresh_technology_list(
    commands: &mut Commands,
    sim: &factory_sim::Simulation,
    nodes: &mut TechnologyPanelNodes,
) {
    for (button, mut background, _) in &mut nodes.technology_buttons {
        background.0 =
            super::helpers::technology_state_color(technology_ui_state(sim, button.technology_id));
    }
    for (entity, marker) in &nodes.status_labels {
        commands
            .entity(entity)
            .insert(Text::new(technology_status_text(sim, marker.0)));
    }
}

fn refresh_selected_borders(
    previous: Option<factory_data::TechnologyId>,
    next: Option<factory_data::TechnologyId>,
    nodes: &mut TechnologyPanelNodes,
) {
    for (button, _, mut border) in &mut nodes.technology_buttons {
        if previous == Some(button.technology_id) || next == Some(button.technology_id) {
            border.set_all(if next == Some(button.technology_id) {
                Color::srgb(0.94, 0.66, 0.20)
            } else {
                Color::srgba(0.32, 0.33, 0.31, 0.80)
            });
        }
    }
}

fn set_detail_text(
    commands: &mut Commands,
    labels: &Query<(Entity, &TechnologyDetailText)>,
    field: TechnologyDetailField,
    value: String,
) {
    for (entity, marker) in labels {
        if marker.0 == field {
            commands.entity(entity).insert(Text::new(value));
            return;
        }
    }
}

fn refresh_start_button(
    sim: &factory_sim::Simulation,
    selected: factory_data::TechnologyId,
    nodes: &mut TechnologyPanelNodes,
) {
    let actionable = can_enqueue_for_ui(sim, selected);
    for (mut background, mut border) in &mut nodes.start_buttons {
        background.0 = if actionable {
            Color::srgba(0.20, 0.34, 0.28, 0.98)
        } else {
            Color::srgba(0.09, 0.095, 0.095, 0.96)
        };
        border.set_all(if actionable {
            Color::srgba(0.42, 0.68, 0.48, 0.85)
        } else {
            Color::srgba(0.25, 0.26, 0.25, 0.85)
        });
    }
}

fn set_marked_text<M: Component>(
    commands: &mut Commands,
    markers: &Query<Entity, With<M>>,
    value: String,
) {
    for entity in markers {
        commands.entity(entity).insert(Text::new(value.clone()));
    }
}

fn reconcile_research_queue(
    commands: &mut Commands,
    sim: &factory_sim::Simulation,
    nodes: &mut TechnologyPanelNodes,
) {
    let Some((root, current_children)) = nodes.queue_roots.iter().next() else {
        return;
    };
    let title = nodes.queue_titles.iter().next();
    let empty = nodes
        .queue_empty
        .iter_mut()
        .next()
        .map(|(entity, mut node, mut visibility)| {
            let visible = sim.research_queue().is_empty();
            node.display = retained_display(visible);
            *visibility = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            entity
        });
    let mut ordered = Vec::with_capacity(sim.research_queue().len() + 2);
    ordered.extend(title);
    ordered.extend(empty);

    for (index, technology_id) in sim.research_queue().iter().copied().enumerate() {
        if let Some((entity, _, children)) = nodes
            .queue_rows
            .iter()
            .find(|(_, marker, _)| marker.0 == technology_id)
        {
            if let Some(label) = children.first() {
                commands.entity(*label).insert(Text::new(format!(
                    "{}. {}",
                    index + 1,
                    super::helpers::technology_name(sim.catalog(), technology_id)
                )));
            }
            for child in children.iter().skip(1) {
                if let Ok((mut button, mut node, mut visibility)) =
                    nodes.queue_buttons.get_mut(child)
                {
                    button.index = index;
                    let enabled = match button.action {
                        TechnologyQueueAction::MoveUp => {
                            index > 0 && sim.can_move_queued_research(index, index - 1).is_ok()
                        }
                        TechnologyQueueAction::MoveDown => {
                            index + 1 < sim.research_queue().len()
                                && sim.can_move_queued_research(index, index + 1).is_ok()
                        }
                        TechnologyQueueAction::Remove => true,
                    };
                    node.display = retained_display(enabled);
                    *visibility = if enabled {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_technology_queue_row(
                    parent,
                    sim,
                    index,
                    technology_id,
                ));
            });
        }
    }
    for (entity, marker, _) in &nodes.queue_rows {
        if !sim.research_queue().contains(&marker.0) {
            commands.entity(entity).despawn();
        }
    }
    replace_children_if_different(commands, root, current_children, &ordered);
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::technology_id_by_name;

    #[derive(Resource, Default)]
    struct PanelChangeCounts {
        active_labels: usize,
        detail_labels: Vec<TechnologyDetailField>,
        progress_fills: usize,
        status_labels: usize,
        technology_buttons: usize,
    }

    type ChangedTechnologyButtonQuery<'w, 's> = Query<
        'w,
        's,
        (),
        (
            With<TechnologySelectButton>,
            Or<(Changed<BackgroundColor>, Changed<BorderColor>)>,
        ),
    >;

    fn count_panel_changes(
        active_labels: Query<(), (With<ActiveResearchText>, Changed<Text>)>,
        detail_labels: Query<(&TechnologyDetailText, Ref<Text>)>,
        progress_fills: Query<(), (With<TechnologyProgressFill>, Changed<Node>)>,
        status_labels: Query<(), (With<TechnologyStatusText>, Changed<Text>)>,
        technology_buttons: ChangedTechnologyButtonQuery,
        mut counts: ResMut<PanelChangeCounts>,
    ) {
        counts.active_labels = active_labels.iter().count();
        counts.detail_labels = detail_labels
            .iter()
            .filter(|(_, text)| text.is_changed())
            .map(|(marker, _)| marker.0)
            .collect();
        counts.progress_fills = progress_fills.iter().count();
        counts.status_labels = status_labels.iter().count();
        counts.technology_buttons = technology_buttons.iter().count();
    }

    #[test]
    fn closed_panel_does_not_access_uninitialized_simulation() {
        let mut app = App::new();
        app.insert_resource(SimResource::empty())
            .init_resource::<TechnologyWindowState>()
            .add_systems(Update, sync_technology_panel);

        app.update();
    }

    #[test]
    fn in_progress_research_only_changes_three_panel_nodes() {
        let mut simulation = factory_sim::Simulation::new_test_world(123);
        let technology_id = technology_id_by_name(simulation.catalog(), "logistics");
        simulation
            .select_research(technology_id)
            .expect("logistics should be researchable");

        let mut app = App::new();
        app.insert_resource(SimResource::new(simulation))
            .insert_resource(TechnologyWindowState {
                open: true,
                selected: Some(technology_id),
            })
            .init_resource::<PanelChangeCounts>()
            .add_systems(Update, (sync_technology_panel, count_panel_changes).chain());
        app.update();

        app.world_mut()
            .resource_mut::<SimResource>()
            .write_for_tests()
            .add_research_units(1)
            .expect("one unit should advance research");
        app.update();

        let counts = app.world().resource::<PanelChangeCounts>();
        assert_eq!(counts.active_labels, 1);
        assert_eq!(counts.detail_labels, vec![TechnologyDetailField::Progress]);
        assert_eq!(counts.progress_fills, 1);
        assert_eq!(counts.status_labels, 0);
        assert_eq!(counts.technology_buttons, 0);
    }
}
