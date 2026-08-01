//! Signal picker for the circuit editors: the shared grid, wired to the slot
//! one of their buttons opened it for.

use bevy::prelude::*;
use factory_sim::SignalId;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::OpenContainer;
use crate::ui::window_sync::{WindowRootQuery, sync_window};

use crate::ui::signal_picker::{SignalFilter, signal_picker_root, spawn_signal_picker_contents};

use super::interaction::command_for_picked_signal;
use super::state::{CircuitEditorState, SignalSlot};

/// `None` clears the slot; `Some` assigns that signal.
#[derive(Component)]
pub(crate) struct SignalPickerButton(pub(crate) Option<SignalId>);

/// The picker rebuilds only when the slot it targets changes; the signal list
/// itself is fixed for a given catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SignalPickerSnapshot {
    slot: SignalSlot,
}

pub(crate) fn sync_signal_picker(
    mut commands: Commands,
    sim: Res<SimResource>,
    editor: Res<CircuitEditorState>,
    open_container: Res<OpenContainer>,
    mut roots: WindowRootQuery<SignalPickerSnapshot>,
) {
    // The picker always edits the open entity, so it closes with the window.
    let Some(slot) = open_container.entity_id.and(editor.picker) else {
        for (entity, _, _) in roots.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };
    sync_window(
        &mut commands,
        &mut roots,
        true,
        editor.is_changed() || open_container.is_changed(),
        || SignalPickerSnapshot { slot },
        signal_picker_root,
        |root, snapshot| {
            spawn_signal_picker_contents(
                root,
                sim.read().catalog(),
                if snapshot.slot.is_item_only() {
                    SignalFilter::ItemsOnly
                } else {
                    SignalFilter::Any
                },
                // Operand slots can hold a number instead, so clearing them
                // means "back to a constant" rather than "unset".
                if snapshot.slot.is_operand() {
                    "Use a number"
                } else {
                    "Clear"
                },
                SignalPickerButton,
            );
        },
    );
}

pub(crate) fn handle_signal_picker_buttons(
    mut buttons: Query<(&Interaction, &SignalPickerButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<CircuitEditorState>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some(signal) = buttons
        .iter_mut()
        .find(|(interaction, _)| **interaction == Interaction::Pressed)
        .map(|(_, button)| button.0)
    else {
        return;
    };
    let (Some(entity_id), Some(slot)) = (open_container.entity_id, editor.picker) else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    editor.picker = None;

    let command = {
        let sim = sim.read();
        command_for_picked_signal(&sim, entity_id, slot, signal)
    };
    if let Some(command) = command {
        commands.write(SimCommandRequest(command));
    }
}
