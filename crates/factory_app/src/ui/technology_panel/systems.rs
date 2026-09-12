use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_data::TechnologyId;
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
    active_research_text, can_enqueue_for_ui, next_science_cost_text, prerequisite_text,
    queue_text, start_queue_label, technology_effect_text, technology_panel_snapshot,
    technology_progress_text, technology_ui_state,
};
use super::view::{
    research_progress_percent, spawn_technology_detail, spawn_technology_panel_contents,
    spawn_technology_queue_row, technology_panel_root, technology_status_text,
};
use crate::ui::formatting::format_recipe_display_name;
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
    mut refresh: ResMut<TechnologyPanelRefresh>,
    mut roots: WindowRootQuery<TechnologyPanelSnapshot>,
    mut nodes: TechnologyPanelNodes,
) {
    let simulation = sim.read();
    let key = TechnologyRefreshKey {
        replacement_revision: sim.replacement_revision(),
        research_revision: simulation.research_revision(),
        selected: window_state.selected,
    };
    let inputs_changed = refresh.last_key != Some(key) || window_state.is_changed();
    if window_state.open {
        refresh.last_key = Some(key);
    }
    sync_retained_window(
        &mut commands,
        &mut roots,
        WindowSyncInput {
            open: window_state.open,
            changed: inputs_changed,
        },
        || technology_panel_snapshot(&simulation, &window_state),
        technology_panel_root,
        |root, _| spawn_technology_panel_contents(root, &simulation, window_state.selected),
        |commands, _, previous, next| {
            update_technology_panel(commands, &simulation, previous, next, &mut nodes);
        },
    );
}

#[derive(Resource, Default)]
pub(crate) struct TechnologyPanelRefresh {
    last_key: Option<TechnologyRefreshKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TechnologyRefreshKey {
    replacement_revision: u64,
    research_revision: u64,
    selected: Option<TechnologyId>,
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
    texts: Query<'w, 's, &'static mut Text>,
}

fn update_technology_panel(
    commands: &mut Commands,
    sim: &factory_sim::Simulation,
    previous: &TechnologyPanelSnapshot,
    next: &TechnologyPanelSnapshot,
    nodes: &mut TechnologyPanelNodes,
) {
    set_marked_text(
        &nodes.active_labels,
        &mut nodes.texts,
        active_research_text(sim),
    );
    set_marked_text(&nodes.queue_labels, &mut nodes.texts, queue_text(sim));

    for (button, mut background, mut border) in &mut nodes.technology_buttons {
        background.0 =
            super::helpers::technology_state_color(technology_ui_state(sim, button.technology_id));
        border.set_all(if next.selected == Some(button.technology_id) {
            Color::srgb(0.94, 0.66, 0.20)
        } else {
            Color::srgba(0.32, 0.33, 0.31, 0.80)
        });
    }
    for (entity, marker) in &nodes.status_labels {
        if let Ok(mut text) = nodes.texts.get_mut(entity) {
            text.0 = technology_status_text(sim, marker.0);
        }
    }

    if previous.selected != next.selected {
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
        for (entity, marker) in &nodes.detail_labels {
            let value = match marker.0 {
                TechnologyDetailField::Name => format_recipe_display_name(&technology.name),
                TechnologyDetailField::Level => format!(
                    "Current level: {} · {}",
                    sim.technology_level(selected).unwrap_or(0),
                    if technology.level_model.is_repeatable() {
                        "Repeatable"
                    } else {
                        "Finite"
                    }
                ),
                TechnologyDetailField::Progress => {
                    format!("Progress: {}", technology_progress_text(sim, selected))
                }
                TechnologyDetailField::Prerequisites => format!(
                    "Prerequisites: {}",
                    prerequisite_text(sim.catalog(), technology)
                ),
                TechnologyDetailField::Cost => {
                    format!("Cost: {}", next_science_cost_text(sim, technology))
                }
                TechnologyDetailField::Effects => {
                    format!("Effects: {}", technology_effect_text(sim, technology))
                }
                TechnologyDetailField::Start => start_queue_label(sim, selected),
            };
            if let Ok(mut text) = nodes.texts.get_mut(entity)
                && text.0 != value
            {
                text.0 = value;
            }
        }
        for mut node in &mut nodes.progress_fills {
            node.width = Val::Percent(research_progress_percent(sim, selected));
        }
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
        reconcile_research_queue(commands, sim, nodes);
    }
}

fn set_marked_text<M: Component>(
    markers: &Query<Entity, With<M>>,
    texts: &mut Query<&mut Text>,
    value: String,
) {
    for entity in markers {
        if let Ok(mut text) = texts.get_mut(entity)
            && text.0 != value
        {
            text.0.clone_from(&value);
        }
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
            if let Some(label) = children.first()
                && let Ok(mut text) = nodes.texts.get_mut(*label)
            {
                text.0 = format!(
                    "{}. {}",
                    index + 1,
                    super::helpers::technology_name(sim.catalog(), technology_id)
                );
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
