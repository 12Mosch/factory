use bevy::prelude::*;
use factory_sim::SimCommand;

use crate::audio::SoundEvent;
use crate::resources::{CraftingWindowState, SimResource};
use crate::simulation::SimCommandRequest;

use super::components::{
    CraftingPanelSnapshot, CraftingQueueSnapshot, CraftingRecipeButton, CraftingTabButton,
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
        if craftable_for_player(&sim.sim, button.recipe_id) {
            commands.write(SimCommandRequest(SimCommand::StartManualCraft(
                button.recipe_id,
            )));
        }
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
        queue_snapshot(&sim.sim)
    } else {
        Vec::new()
    };
    let result = sync_window(
        &mut commands,
        &mut roots,
        state.open,
        true,
        || crafting_panel_snapshot(&sim.sim, state.selected_tab),
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
