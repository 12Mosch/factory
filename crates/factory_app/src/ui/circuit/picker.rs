//! Signal picker: a scrollable grid of every signal in the catalog, opened by
//! a slot button in one of the circuit editors.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_sim::SignalId;

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::OpenContainer;
use crate::ui::window_sync::{WindowRootQuery, sync_window};

use super::interaction::command_for_picked_signal;
use super::signals::{SignalCatalog, signal_short_label};
use super::state::{CircuitEditorState, SignalSlot};
use super::widgets::BUTTON_BACKGROUND;

const CELL_WIDTH: f32 = 42.0;
const CELL_HEIGHT: f32 = 20.0;

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
    let slot = open_container.entity_id.and(editor.picker);
    sync_window(
        &mut commands,
        &mut roots,
        slot.is_some(),
        editor.is_changed() || open_container.is_changed(),
        || SignalPickerSnapshot {
            slot: slot.expect("snapshot is only built while the picker is open"),
        },
        picker_root,
        |root, snapshot| spawn_picker_contents(root, &sim.read(), snapshot.slot),
    );
}

fn picker_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Px(320.0),
            max_height: Val::Vh(50.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.98)),
        GlobalZIndex(1200),
    )
}

fn spawn_picker_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    slot: SignalSlot,
) {
    root.spawn((
        Text::new("Pick a signal"),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
    ));
    // Operand slots can hold a number instead, so clearing them means "back to
    // a constant" rather than "unset".
    let clear_label = if slot.is_operand() {
        "Use a number"
    } else {
        "Clear"
    };
    root.spawn((
        Button,
        Node {
            width: Val::Px(110.0),
            height: Val::Px(20.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BUTTON_BACKGROUND),
        SignalPickerButton(None),
    ))
    .with_child((
        Text::new(clear_label.to_string()),
        TextFont::from_font_size(10.0),
        TextColor(Color::WHITE),
    ));

    let catalog = SignalCatalog::from_catalog(sim.catalog());
    root.spawn((
        Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            overflow: Overflow::scroll_y(),
            scrollbar_width: 10.0,
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.022, 0.023, 0.75)),
        ScrollArea,
    ))
    .with_children(|viewport| {
        for (heading, signals) in [
            ("Items", &catalog.items),
            ("Fluids", &catalog.fluids),
            ("Signals", &catalog.virtuals),
        ] {
            if signals.is_empty() {
                continue;
            }
            viewport.spawn((
                Text::new(heading.to_string()),
                TextFont::from_font_size(10.0),
                TextColor(Color::srgb(0.70, 0.74, 0.80)),
            ));
            viewport
                .spawn((
                    Node {
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(3.0),
                        row_gap: Val::Px(3.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|grid| {
                    for &signal in signals {
                        grid.spawn((
                            Button,
                            Node {
                                width: Val::Px(CELL_WIDTH),
                                height: Val::Px(CELL_HEIGHT),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(BUTTON_BACKGROUND),
                            SignalPickerButton(Some(signal)),
                        ))
                        .with_child((
                            Text::new(signal_short_label(sim.catalog(), signal)),
                            TextFont::from_font_size(9.0),
                            TextColor(Color::WHITE),
                            TextLayout::justify(Justify::Center),
                        ));
                    }
                });
        }
    });
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
