use bevy::prelude::*;

use crate::resources::{CraftingWindowState, SimResource};

use super::components::{CraftingPanelRoot, CraftingRecipeButton, CraftingTabButton};
use super::helpers::{craftable_for_player, crafting_panel_snapshot};
use super::view::{spawn_manual_crafting_contents, spawn_manual_crafting_panel};

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
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction == Interaction::Pressed {
            state.selected_tab = button.tab;
        }
    }
}

pub(crate) fn handle_manual_crafting_recipe_buttons(
    mut interactions: CraftingRecipeInteractionQuery,
    mut sim: ResMut<SimResource>,
    state: Res<CraftingWindowState>,
) {
    if !state.open {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if craftable_for_player(&sim.sim, button.recipe_id) {
            let _ = sim.sim.start_manual_craft(button.recipe_id);
        }
    }
}

pub(crate) fn sync_manual_crafting_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    state: Res<CraftingWindowState>,
    mut roots: Query<(Entity, &mut CraftingPanelRoot, Option<&Children>)>,
) {
    if !state.open {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let snapshot = crafting_panel_snapshot(&sim.sim, state.selected_tab);
    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        spawn_manual_crafting_panel(&mut commands, snapshot);
        return;
    };
    for (duplicate_entity, _, _) in roots_iter {
        commands.entity(duplicate_entity).despawn();
    }

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
        .with_children(|root| spawn_manual_crafting_contents(root, &snapshot));
}
