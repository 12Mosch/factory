use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::{CraftingError, SimCommand, SimCommandError};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::{SimCommandRequest, SimCommandResult};
use crate::ui::resources::CraftingWindowState;

use super::components::{
    CraftingFeedbackText, CraftingPanelSnapshot, CraftingQueueAction, CraftingQueueButton,
    CraftingQueueEmpty, CraftingQueueProgressFill, CraftingQueueRoot, CraftingQueueRow,
    CraftingRecipeButton, CraftingRecipeEmpty, CraftingRecipeListRoot, CraftingRecipeRow,
    CraftingTabButton, ManualCraftQueueRow, ManualCraftRecipeRow,
};
use super::helpers::{
    CraftingRecipeTextCache, cached_crafting_panel_snapshot, craftable_for_player,
};
use super::view::{
    manual_crafting_root, spawn_manual_crafting_contents, spawn_queue_row, spawn_recipe_row,
};
use crate::ui::window_sync::{
    WindowRootQuery, WindowSyncInput, replace_children_if_different, retained_display,
    sync_retained_window,
};

const CRAFTING_REFRESH_TICKS: u64 = 15;

type CraftingTabInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static CraftingTabButton),
    (Changed<Interaction>, With<Button>),
>;
type CraftingRecipeInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static CraftingRecipeButton),
    (Changed<Interaction>, With<Button>),
>;
type CraftingQueueInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static CraftingQueueButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_manual_crafting_tab_buttons(
    mut interactions: CraftingTabInteractionQuery,
    mut state: ResMut<CraftingWindowState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction == Interaction::Pressed {
            sounds.write(SoundEvent::UiClick);
            state.selected_tab = button.tab;
        }
    }
}

pub(crate) fn handle_manual_crafting_recipe_buttons(
    mut interactions: CraftingRecipeInteractionQuery,
    sim: Res<SimResource>,
    state: Res<CraftingWindowState>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if !state.open || state.selected_tab != crate::ui::resources::CraftingPanelTab::Player {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if craftable_for_player(&sim.read(), button.recipe_id) {
            commands.write(SimCommandRequest(SimCommand::StartManualCraft(
                button.recipe_id,
            )));
        }
    }
}

pub(crate) fn handle_manual_crafting_queue_buttons(
    mut interactions: CraftingQueueInteractionQuery,
    state: Res<CraftingWindowState>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let command = match button.action {
            CraftingQueueAction::Cancel => SimCommand::CancelManualCraft {
                job_id: button.job_id,
            },
            CraftingQueueAction::Move(direction) => SimCommand::MoveManualCraft {
                job_id: button.job_id,
                direction,
            },
        };
        commands.write(SimCommandRequest(command));
    }
}

pub(crate) fn handle_manual_crafting_command_results(
    mut results: MessageReader<SimCommandResult>,
    mut state: ResMut<CraftingWindowState>,
) {
    for outcome in results.read() {
        if !matches!(
            &outcome.command,
            SimCommand::StartManualCraft(_)
                | SimCommand::CancelManualCraft { .. }
                | SimCommand::MoveManualCraft { .. }
        ) {
            continue;
        }

        state.feedback = match outcome.result {
            Ok(_) => None,
            Err(SimCommandError::Crafting(CraftingError::RefundInventoryFull)) => {
                Some("Cannot cancel: make inventory room for all refunded ingredients.".to_string())
            }
            Err(SimCommandError::Crafting(CraftingError::MissingJob(_))) => {
                Some("That crafting job is no longer queued.".to_string())
            }
            Err(SimCommandError::Crafting(CraftingError::InsufficientIngredients)) => {
                Some("Not enough ingredients to start that craft.".to_string())
            }
            Err(SimCommandError::Crafting(_)) | Err(_) => {
                Some("The crafting queue could not be changed.".to_string())
            }
        };
    }
}

pub(crate) fn sync_manual_crafting_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    state: Res<CraftingWindowState>,
    mut refresh: ResMut<ManualCraftingRefresh>,
    mut recipe_text: ResMut<CraftingRecipeTextCache>,
    mut roots: WindowRootQuery<CraftingPanelSnapshot>,
    mut nodes: ManualCraftingNodes,
) {
    let simulation = sim.read();
    if state.open {
        recipe_text.refresh(&simulation, sim.replacement_revision());
    }
    let key = ManualCraftingRefreshKey {
        replacement_revision: sim.replacement_revision(),
        tick_bucket: simulation.tick_count() / CRAFTING_REFRESH_TICKS,
        crafting_revision: simulation.crafting_revision(),
        selected_tab: state.selected_tab,
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
        || {
            cached_crafting_panel_snapshot(
                &simulation,
                state.selected_tab,
                state.feedback.clone(),
                &recipe_text,
            )
        },
        manual_crafting_root,
        spawn_manual_crafting_contents,
        |commands, _, _, next| {
            update_manual_crafting(commands, next, &mut nodes);
        },
    );
}

#[derive(Resource, Default)]
pub(crate) struct ManualCraftingRefresh {
    last_key: Option<ManualCraftingRefreshKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ManualCraftingRefreshKey {
    replacement_revision: u64,
    tick_bucket: u64,
    crafting_revision: u64,
    selected_tab: crate::ui::resources::CraftingPanelTab,
}

type FeedbackFilter = (
    With<CraftingFeedbackText>,
    Without<CraftingRecipeEmpty>,
    Without<CraftingQueueEmpty>,
    Without<CraftingQueueButton>,
    Without<CraftingQueueProgressFill>,
);
type FeedbackQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static mut Node, &'static mut Visibility), FeedbackFilter>;
type CraftingTabFilter = (
    With<CraftingTabButton>,
    Without<CraftingRecipeRow>,
    Without<CraftingRecipeButton>,
);
type CraftingTabStyleQuery<'w, 's> =
    Query<'w, 's, (&'static CraftingTabButton, &'static mut BackgroundColor), CraftingTabFilter>;
type RecipeEmptyFilter = (
    With<CraftingRecipeEmpty>,
    Without<CraftingFeedbackText>,
    Without<CraftingQueueEmpty>,
    Without<CraftingQueueButton>,
    Without<CraftingQueueProgressFill>,
);
type RecipeEmptyQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static mut Node, &'static mut Visibility), RecipeEmptyFilter>;
type RecipeRowQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CraftingRecipeRow,
        &'static Children,
        &'static mut BackgroundColor,
    ),
    (With<CraftingRecipeRow>, Without<CraftingRecipeButton>),
>;
type RecipeButtonQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Children,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (With<CraftingRecipeButton>, Without<CraftingRecipeRow>),
>;
type QueueEmptyFilter = (
    With<CraftingQueueEmpty>,
    Without<CraftingFeedbackText>,
    Without<CraftingRecipeEmpty>,
    Without<CraftingQueueButton>,
    Without<CraftingQueueProgressFill>,
);
type QueueEmptyQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static mut Node, &'static mut Visibility), QueueEmptyFilter>;
type QueueButtonFilter = (
    Without<CraftingFeedbackText>,
    Without<CraftingRecipeEmpty>,
    Without<CraftingQueueEmpty>,
    Without<CraftingQueueProgressFill>,
);
type QueueButtonQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut CraftingQueueButton,
        &'static mut Node,
        &'static mut Visibility,
    ),
    QueueButtonFilter,
>;
type QueueProgressFillQuery<'w, 's> = Query<
    'w,
    's,
    (&'static CraftingQueueProgressFill, &'static mut Node),
    (
        Without<CraftingFeedbackText>,
        Without<CraftingRecipeEmpty>,
        Without<CraftingQueueEmpty>,
        Without<CraftingQueueButton>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct ManualCraftingNodes<'w, 's> {
    feedback: FeedbackQuery<'w, 's>,
    tabs: CraftingTabStyleQuery<'w, 's>,
    recipe_roots: Query<'w, 's, (Entity, &'static Children), With<CraftingRecipeListRoot>>,
    recipe_empty: RecipeEmptyQuery<'w, 's>,
    recipe_rows: RecipeRowQuery<'w, 's>,
    recipe_buttons: RecipeButtonQuery<'w, 's>,
    queue_roots: Query<'w, 's, (Entity, &'static Children), With<CraftingQueueRoot>>,
    queue_empty: QueueEmptyQuery<'w, 's>,
    queue_rows: Query<'w, 's, (Entity, &'static CraftingQueueRow, &'static Children)>,
    queue_buttons: QueueButtonQuery<'w, 's>,
    progress_fills: QueueProgressFillQuery<'w, 's>,
    children: Query<'w, 's, &'static Children>,
    texts: Query<'w, 's, &'static mut Text>,
    text_fonts: Query<'w, 's, &'static mut TextFont>,
    text_colors: Query<'w, 's, &'static mut TextColor>,
}

fn update_manual_crafting(
    commands: &mut Commands,
    snapshot: &CraftingPanelSnapshot,
    nodes: &mut ManualCraftingNodes,
) {
    for (entity, mut node, mut visibility) in &mut nodes.feedback {
        let visible = snapshot.feedback.is_some();
        node.display = retained_display(visible);
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Ok(mut text) = nodes.texts.get_mut(entity) {
            text.0 = snapshot.feedback.clone().unwrap_or_default();
        }
    }
    for (button, mut background) in &mut nodes.tabs {
        background.0 = if button.tab == snapshot.selected_tab {
            Color::srgba(0.22, 0.27, 0.24, 0.98)
        } else {
            Color::srgba(0.10, 0.11, 0.11, 0.98)
        };
    }
    reconcile_recipe_rows(commands, &snapshot.rows, nodes);
    reconcile_queue_rows(commands, &snapshot.queue, nodes);
}

fn reconcile_recipe_rows(
    commands: &mut Commands,
    rows: &[ManualCraftRecipeRow],
    nodes: &mut ManualCraftingNodes,
) {
    let Some((root, current_children)) = nodes.recipe_roots.iter().next() else {
        return;
    };
    let empty = nodes
        .recipe_empty
        .iter_mut()
        .next()
        .map(|(entity, mut node, mut visibility)| {
            let visible = rows.is_empty();
            node.display = retained_display(visible);
            *visibility = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            entity
        });
    let mut ordered = Vec::with_capacity(rows.len() + 1);
    ordered.extend(empty);
    for row in rows {
        if let Some((entity, _, children, mut row_background)) = nodes
            .recipe_rows
            .iter_mut()
            .find(|(_, marker, _, _)| marker.0 == row.recipe_id)
        {
            row_background.0 = if row.button_enabled {
                Color::srgba(0.070, 0.077, 0.073, 0.94)
            } else {
                Color::srgba(0.050, 0.052, 0.052, 0.90)
            };
            if let Some(details) = children.first()
                && let Ok(detail_children) = nodes.children.get(*details)
            {
                let values = [
                    row.display_name.as_ref(),
                    row.products.as_ref(),
                    row.ingredients.as_str(),
                ];
                for (text_entity, value) in detail_children.iter().zip(values) {
                    if let Ok(mut text) = nodes.texts.get_mut(text_entity)
                        && text.0 != value
                    {
                        text.0 = value.to_owned();
                    }
                }
            }
            if let Some(button_entity) = children.get(1)
                && let Ok((button_children, mut background, mut border)) =
                    nodes.recipe_buttons.get_mut(*button_entity)
            {
                background.0 = if row.button_enabled {
                    Color::srgba(0.18, 0.34, 0.25, 0.98)
                } else {
                    Color::srgba(0.09, 0.095, 0.095, 0.96)
                };
                border.set_all(if row.button_enabled {
                    Color::srgba(0.42, 0.55, 0.43, 0.90)
                } else {
                    Color::srgba(0.25, 0.26, 0.25, 0.85)
                });
                if let Some(label) = button_children.first() {
                    if let Ok(mut text) = nodes.texts.get_mut(*label) {
                        text.0.clone_from(&row.status);
                    }
                    if let Ok(mut font) = nodes.text_fonts.get_mut(*label) {
                        font.font_size = FontSize::Px(if row.button_enabled { 11.0 } else { 10.0 });
                    }
                    if let Ok(mut color) = nodes.text_colors.get_mut(*label) {
                        color.0 = if row.button_enabled {
                            Color::WHITE
                        } else {
                            Color::srgb(0.72, 0.74, 0.70)
                        };
                    }
                }
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_recipe_row(parent, row));
            });
        }
    }
    for (entity, marker, _, _) in &nodes.recipe_rows {
        if !rows.iter().any(|row| row.recipe_id == marker.0) {
            commands.entity(entity).despawn();
        }
    }
    replace_children_if_different(commands, root, current_children, &ordered);
}

fn reconcile_queue_rows(
    commands: &mut Commands,
    rows: &[ManualCraftQueueRow],
    nodes: &mut ManualCraftingNodes,
) {
    let Some((root, current_children)) = nodes.queue_roots.iter().next() else {
        return;
    };
    let empty = nodes
        .queue_empty
        .iter_mut()
        .next()
        .map(|(entity, mut node, mut visibility)| {
            let visible = rows.is_empty();
            node.display = retained_display(visible);
            *visibility = if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            entity
        });
    let mut ordered = Vec::with_capacity(rows.len() + 1);
    ordered.extend(empty);
    for row in rows {
        if let Some((entity, _, children)) = nodes
            .queue_rows
            .iter()
            .find(|(_, marker, _)| marker.0 == row.job_id)
        {
            if let Some(label) = children.first()
                && let Ok(mut text) = nodes.texts.get_mut(*label)
            {
                text.0.clone_from(&row.status);
            }
            for (marker, mut node) in &mut nodes.progress_fills {
                if marker.0 == row.job_id {
                    node.width = Val::Percent(f32::from(row.progress_percent));
                }
            }
            for child in children.iter().skip(2) {
                if let Ok((mut button, mut node, mut visibility)) =
                    nodes.queue_buttons.get_mut(child)
                {
                    button.job_id = row.job_id;
                    let visible = match button.action {
                        CraftingQueueAction::Move(factory_sim::CraftingQueueMove::Earlier) => {
                            row.can_move_earlier
                        }
                        CraftingQueueAction::Move(factory_sim::CraftingQueueMove::Later) => {
                            row.can_move_later
                        }
                        CraftingQueueAction::Cancel => true,
                    };
                    node.display = retained_display(visible);
                    *visibility = if visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
            ordered.push(entity);
        } else {
            commands.entity(root).with_children(|parent| {
                ordered.push(spawn_queue_row(parent, row));
            });
        }
    }
    for (entity, marker, _) in &nodes.queue_rows {
        if !rows.iter().any(|row| row.job_id == marker.0) {
            commands.entity(entity).despawn();
        }
    }
    replace_children_if_different(commands, root, current_children, &ordered);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use factory_data::{item_id_by_name, recipe_id_by_name};
    use factory_sim::Simulation;

    #[test]
    fn assembling_tab_recipe_button_does_not_queue_manual_craft() {
        let mut sim = Simulation::new_test_world(123);
        let catalog = sim.catalog().clone();
        let iron_plate = item_id_by_name(&catalog, "iron_plate");
        let gear = recipe_id_by_name(&catalog, "iron_gear_wheel");
        sim.player_inventory_mut()
            .insert(&catalog, iron_plate, 2)
            .unwrap();

        let state = CraftingWindowState {
            open: true,
            selected_tab: crate::ui::resources::CraftingPanelTab::Assembling,
            ..default()
        };
        let mut app = App::new();
        app.insert_resource(SimResource::new(sim))
            .insert_resource(state)
            .add_message::<SimCommandRequest>()
            .add_systems(Update, handle_manual_crafting_recipe_buttons);
        app.world_mut().spawn((
            Button,
            Interaction::Pressed,
            CraftingRecipeButton { recipe_id: gear },
        ));

        app.update();

        assert!(
            app.world_mut()
                .resource_mut::<Messages<SimCommandRequest>>()
                .drain()
                .next()
                .is_none()
        );
    }
}
