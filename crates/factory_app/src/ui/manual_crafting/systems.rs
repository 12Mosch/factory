use bevy::prelude::*;
use factory_sim::{CraftingError, SimCommand, SimCommandError};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::{SimCommandRequest, SimCommandResult};
use crate::ui::resources::CraftingWindowState;

use super::components::{
    CraftingPanelSnapshot, CraftingQueueAction, CraftingQueueButton, CraftingQueueSnapshot,
    CraftingRecipeButton, CraftingTabButton,
};
use super::helpers::{craftable_for_player, crafting_panel_snapshot, queue_snapshot};
use super::view::{manual_crafting_root, spawn_manual_crafting_contents, spawn_queue_contents};
use crate::ui::window_sync::{WindowRootQuery, WindowSync, sync_contents, sync_window};

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
    if !state.open {
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
    mut roots: WindowRootQuery<CraftingPanelSnapshot>,
    mut queue_roots: WindowRootQuery<CraftingQueueSnapshot>,
) {
    let queue = if state.open {
        queue_snapshot(&sim.read())
    } else {
        Vec::new()
    };
    let result = sync_window(
        &mut commands,
        &mut roots,
        state.open,
        true,
        || crafting_panel_snapshot(&sim.read(), state.selected_tab, state.feedback.clone()),
        manual_crafting_root,
        |root, snapshot| spawn_manual_crafting_contents(root, snapshot, queue.clone()),
    );
    // When the panel itself was rebuilt the queue root was respawned with
    // fresh contents; only an unchanged panel needs the inner sync.
    if result == WindowSync::Unchanged {
        sync_contents(
            &mut commands,
            &mut queue_roots,
            CraftingQueueSnapshot(queue),
            |queue_node, snapshot| spawn_queue_contents(queue_node, &snapshot.0),
        );
    }
}
