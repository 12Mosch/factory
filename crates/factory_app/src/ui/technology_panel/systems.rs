use bevy::prelude::*;

use crate::audio::SoundEvent;
use crate::input::panels::escape_consumed;
use crate::resources::{AppInputState, BuildPlacementState, SimResource, TechnologyWindowState};

use super::components::{
    TechnologyPanelRoot, TechnologyQueueAction, TechnologyQueueButton, TechnologySelectButton,
    TechnologyStartQueueButton,
};
use super::helpers::technology_panel_snapshot;
use super::view::{spawn_technology_panel, spawn_technology_panel_contents};

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
    input_state: Option<Res<AppInputState>>,
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

    if !escape_consumed(input_state.as_deref())
        && window_state.open
        && keyboard.just_pressed(KeyCode::Escape)
    {
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
        .is_some_and(|technology_id| sim.sim.catalog().technology(technology_id).is_some())
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
    mut sounds: MessageWriter<SoundEvent>,
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
        if sim.sim.active_research() == Some(technology_id)
            || sim.sim.research_queue().contains(&technology_id)
        {
            continue;
        }

        if sim.sim.enqueue_research(technology_id).is_ok() {
            sounds.write(SoundEvent::UiClick);
        }
    }

    for (interaction, button) in &mut queue_buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match button.action {
            TechnologyQueueAction::Remove => {
                if sim.sim.remove_queued_research(button.index).is_ok() {
                    sounds.write(SoundEvent::UiClick);
                }
            }
            TechnologyQueueAction::MoveUp => {
                if button.index > 0
                    && sim
                        .sim
                        .move_queued_research(button.index, button.index - 1)
                        .is_ok()
                {
                    sounds.write(SoundEvent::UiClick);
                }
            }
            TechnologyQueueAction::MoveDown => {
                if sim
                    .sim
                    .move_queued_research(button.index, button.index + 1)
                    .is_ok()
                {
                    sounds.write(SoundEvent::UiClick);
                }
            }
        }
    }
}

pub(crate) fn sync_technology_panel(
    mut commands: Commands,
    sim: Res<SimResource>,
    window_state: Res<TechnologyWindowState>,
    mut roots: Query<(Entity, &mut TechnologyPanelRoot, Option<&Children>)>,
) {
    if !window_state.open {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let snapshot = technology_panel_snapshot(&sim.sim, &window_state);
    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        spawn_technology_panel(&mut commands, &sim.sim, window_state.selected, snapshot);
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
    root.snapshot = snapshot;
    commands.entity(root_entity).with_children(|root| {
        spawn_technology_panel_contents(root, &sim.sim, window_state.selected)
    });
}
