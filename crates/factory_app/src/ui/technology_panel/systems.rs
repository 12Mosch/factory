use bevy::prelude::*;

use crate::resources::{BuildPlacementState, SimResource, TechnologyWindowState};

use super::components::{
    TechnologyPanelRoot, TechnologyQueueAction, TechnologyQueueButton, TechnologySelectButton,
    TechnologyStartQueueButton,
};
use super::helpers::{technology_by_id, technology_panel_snapshot};
use super::view::spawn_technology_panel;

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
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut window_state: ResMut<TechnologyWindowState>,
    mut build_state: ResMut<BuildPlacementState>,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    if keyboard.just_pressed(KeyCode::KeyT) {
        window_state.open = !window_state.open;
        if window_state.open {
            build_state.selected = None;
        }
    }

    if window_state.open && keyboard.just_pressed(KeyCode::Escape) {
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
        .is_some_and(|technology_id| technology_by_id(sim.sim.catalog(), technology_id).is_some())
    {
        return;
    }

    window_state.selected = sim
        .sim
        .active_research()
        .or_else(|| {
            sim.sim
                .catalog()
                .technologies
                .iter()
                .find(|technology| !sim.sim.is_technology_unlocked(technology.id))
                .map(|technology| technology.id)
        })
        .or_else(|| {
            sim.sim
                .catalog()
                .technologies
                .first()
                .map(|technology| technology.id)
        });
}

pub(crate) fn handle_technology_panel_buttons(
    mut select_buttons: TechnologySelectInteractionQuery,
    mut start_buttons: TechnologyStartInteractionQuery,
    mut queue_buttons: TechnologyQueueInteractionQuery,
    mut sim: ResMut<SimResource>,
    mut window_state: ResMut<TechnologyWindowState>,
) {
    if !window_state.open {
        return;
    }

    for (interaction, button) in &mut select_buttons {
        if *interaction == Interaction::Pressed {
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
        if sim.sim.active_research() == Some(technology_id)
            || sim.sim.research_queue().contains(&technology_id)
        {
            continue;
        }

        let _ = sim.sim.enqueue_research(technology_id);
    }

    for (interaction, button) in &mut queue_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button.action {
            TechnologyQueueAction::Remove => {
                let _ = sim.sim.remove_queued_research(button.index);
            }
            TechnologyQueueAction::MoveUp => {
                if button.index > 0 {
                    let _ = sim.sim.move_queued_research(button.index, button.index - 1);
                }
            }
            TechnologyQueueAction::MoveDown => {
                let _ = sim.sim.move_queued_research(button.index, button.index + 1);
            }
        }
    }
}

pub(crate) fn sync_technology_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    window_state: Res<TechnologyWindowState>,
    roots: Query<(Entity, &TechnologyPanelRoot)>,
) {
    if !window_state.open {
        for (entity, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let snapshot = technology_panel_snapshot(&sim.sim, &window_state);
    for (entity, root) in &roots {
        if root.snapshot != snapshot {
            commands.entity(entity).despawn();
        }
    }
    if roots.iter().any(|(_, root)| root.snapshot == snapshot) {
        return;
    }

    spawn_technology_panel(&mut commands, &sim.sim, window_state.selected, snapshot);
}
