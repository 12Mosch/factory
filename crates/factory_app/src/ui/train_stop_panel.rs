//! The window a player opens on a train stop: its name, how many trains it
//! admits, and where that number comes from.
//!
//! A stop is the one entity whose *name* is load-bearing — a schedule asks for
//! a station by name, and two stops sharing one are two platforms of the same
//! station — so renaming is a first-class control here rather than a detail.
//! Everything else the stop can do is circuitry, and that is the shared circuit
//! panel the container window already puts underneath.
//!
//! Built once and refreshed by writing label text, the way the circuit and
//! module panels are: the name is edited a keystroke at a time, and a window
//! that respawned on each one would lose the caret with it.

use bevy::ecs::system::SystemParam;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, TextCursorStyle};
use factory_sim::{EntityId, SimCommand, Simulation};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::circuit::signals::optional_signal_label;
use crate::ui::circuit::state::{CircuitEditorState, SignalSlot};
use crate::ui::circuit::widgets::{
    LABEL_COLOR, spawn_button, spawn_caption, spawn_heading, spawn_row,
};
use crate::ui::resources::OpenContainer;
use crate::ui::text_input::{editor_value, is_non_control, set_editor_value, single_line_editor};

/// Longest station name a player may type. Long enough for "Iron ore unload
/// west", short enough that the schedule rows it appears in stay readable.
const MAX_STOP_NAME_LENGTH: usize = 40;

/// What a live text node in the stop panel displays.
///
/// One keyed component rather than a marker per field, for the reason the
/// circuit panel's labels give: Bevy proves query disjointness from filters, so
/// separate markers on `Text` nodes would make the refresh queries conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrainStopLabelKind {
    Name,
    TrainLimit,
    LimitSignal,
    Status,
}

#[derive(Component)]
pub(crate) struct TrainStopLabel(pub(crate) TrainStopLabelKind);

/// Starts or finishes typing a new name.
#[derive(Component)]
pub(crate) struct TrainStopRenameButton;

#[derive(Component)]
pub(crate) struct TrainStopNameInput;

type StopLabelQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Text,
        &'static TrainStopLabel,
        &'static mut Node,
    ),
    (With<TrainStopLabel>, Without<TrainStopNameInput>),
>;

type StopNameInputQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut EditableText, &'static mut Node),
    (With<TrainStopNameInput>, Without<TrainStopLabel>),
>;

#[derive(Component)]
pub(crate) struct TrainStopLimitStepButton(pub(crate) i32);

/// Opens the signal picker for the channel the limit is read from.
#[derive(Component)]
pub(crate) struct TrainStopLimitSignalButton;

/// The rename in progress, if any.
///
/// The buffer is separate from the stop's own name because a half-typed name is
/// not a name: schedules point at stations by name, so committing every
/// keystroke would rewrite every schedule serving this station on each letter.
#[derive(Resource, Default)]
pub(crate) struct TrainStopRenameState {
    pub(crate) editing: Option<EntityId>,
    pub(crate) buffer: String,
}

impl TrainStopRenameState {
    fn cancel(&mut self) {
        self.editing = None;
        self.buffer.clear();
    }
}

pub(crate) fn spawn_train_stop_panel(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|panel| {
            spawn_heading(panel, "Train stop");
            spawn_row(panel, |row| {
                row.spawn((
                    Text::new(String::new()),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                    TrainStopLabel(TrainStopLabelKind::Name),
                ));
                row.spawn((
                    Node {
                        display: Display::None,
                        width: Val::Px(190.0),
                        height: Val::Px(26.0),
                        padding: UiRect::horizontal(Val::Px(6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        overflow: Overflow::clip_x(),
                        ..default()
                    },
                    single_line_editor("", Some(MAX_STOP_NAME_LENGTH)),
                    EditableTextFilter::new(is_non_control),
                    TextLayout::no_wrap(),
                    TextCursorStyle::default(),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                    BackgroundColor(Color::srgb(0.045, 0.055, 0.050)),
                    BorderColor::all(Color::srgb(0.34, 0.42, 0.31)),
                    TrainStopNameInput,
                ));
            });
            spawn_row(panel, |row| {
                spawn_button(row, 66.0, "Rename", TrainStopRenameButton, ());
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Trains");
                spawn_button(row, 24.0, "-", TrainStopLimitStepButton(-1), ());
                row.spawn((
                    Text::new(String::new()),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::WHITE),
                    TrainStopLabel(TrainStopLabelKind::TrainLimit),
                ));
                spawn_button(row, 24.0, "+", TrainStopLimitStepButton(1), ());
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Limit from");
                spawn_button(
                    row,
                    56.0,
                    "--",
                    TrainStopLimitSignalButton,
                    TrainStopLabel(TrainStopLabelKind::LimitSignal),
                );
            });
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                TrainStopLabel(TrainStopLabelKind::Status),
            ));
        });
}

/// Writes the live text of the open stop's panel.
pub(crate) fn update_train_stop_panel(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    rename: Res<TrainStopRenameState>,
    mut labels: StopLabelQuery,
    mut inputs: StopNameInputQuery,
) {
    if labels.is_empty() {
        return;
    }
    let sim = sim.read();
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    let Some(state) = sim.train_stop(entity_id) else {
        return;
    };
    let editing = rename.editing == Some(entity_id);
    for (mut text, label, mut node) in &mut labels {
        if label.0 == TrainStopLabelKind::Name {
            node.display = if editing {
                Display::None
            } else {
                Display::Flex
            };
        }
        let value = match label.0 {
            TrainStopLabelKind::Name => state.name.clone(),
            TrainStopLabelKind::TrainLimit => sim.train_stop_effective_limit(entity_id).to_string(),
            TrainStopLabelKind::LimitSignal => {
                optional_signal_label(sim.catalog(), state.train_limit_signal)
            }
            TrainStopLabelKind::Status => stop_status(&sim, entity_id),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    for (mut input, mut node) in &mut inputs {
        node.display = if editing {
            Display::Flex
        } else {
            Display::None
        };
        if editing {
            set_editor_value(&mut input, &rename.buffer);
        }
    }
}

/// The line under the controls: what the stop is doing, in the words a player
/// would use to ask about it.
fn stop_status(sim: &Simulation, entity_id: EntityId) -> String {
    if sim.train_stop_target(entity_id).is_none() {
        return "No track beside it: no train can be sent here".to_string();
    }
    let booked = sim
        .trains()
        .filter(|train| train.scheduled_stop == Some(entity_id))
        .count();
    let waiting = sim.trains().any(|train| {
        train.scheduled_stop == Some(entity_id) && train.is_waiting_at_scheduled_stop()
    });
    let limit = sim.train_stop_effective_limit(entity_id);
    // A station admitting nobody is worth saying outright rather than leaving
    // the player to read it out of "0 of 0": it is what a circuit condition or
    // a limit channel reading zero looks like from here.
    if limit == 0 && !waiting {
        return "Closed: no train will be sent here".to_string();
    }
    let booked = format!("{booked} of {limit} booked");
    if waiting {
        format!("{booked}, one standing here")
    } else {
        booked
    }
}

fn pressed(interaction: &Interaction) -> bool {
    *interaction == Interaction::Pressed
}

/// Starts a rename, or commits the one in progress.
#[derive(SystemParam)]
pub(crate) struct TrainStopRenameButtonState<'w, 's> {
    sim: Res<'w, SimResource>,
    open_container: Res<'w, OpenContainer>,
    rename: ResMut<'w, TrainStopRenameState>,
    inputs: Query<'w, 's, (Entity, &'static mut EditableText), With<TrainStopNameInput>>,
    input_focus: Option<ResMut<'w, InputFocus>>,
    commands: MessageWriter<'w, SimCommandRequest>,
    sounds: MessageWriter<'w, SoundEvent>,
}

pub(crate) fn handle_train_stop_rename_button(
    buttons: Query<&Interaction, (Changed<Interaction>, With<TrainStopRenameButton>)>,
    mut state: TrainStopRenameButtonState,
) {
    if !buttons.iter().any(pressed) {
        return;
    }
    let Some(entity_id) = state.open_container.entity_id else {
        return;
    };
    state.sounds.write(SoundEvent::UiClick);
    let Ok((input_entity, mut input)) = state.inputs.single_mut() else {
        return;
    };
    if state.rename.editing == Some(entity_id) {
        commit_rename(
            &mut state.rename,
            entity_id,
            editor_value(&input),
            &mut state.commands,
        );
        if let Some(focus) = state.input_focus.as_deref_mut() {
            focus.clear();
        }
        return;
    }
    // Seeded with the current name, so a small correction is a small edit
    // rather than retyping the station.
    state.rename.buffer = state
        .sim
        .read()
        .train_stop(entity_id)
        .map(|state| state.name.clone())
        .unwrap_or_default();
    state.rename.editing = Some(entity_id);
    set_editor_value(&mut input, &state.rename.buffer);
    if let Some(focus) = state.input_focus.as_deref_mut() {
        focus.set(input_entity, FocusCause::Navigated);
    }
}

pub(crate) fn sync_train_stop_rename_to_state(
    inputs: Query<&EditableText, (With<TrainStopNameInput>, Changed<EditableText>)>,
    mut rename: ResMut<TrainStopRenameState>,
) {
    if rename.editing.is_none() {
        return;
    }
    for input in &inputs {
        let value = editor_value(input);
        if rename.buffer != value {
            rename.buffer = value;
        }
    }
}

/// Enter commits the focused editor. Closing the stop window abandons it.
pub(crate) fn submit_train_stop_rename(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut input_focus: Option<ResMut<InputFocus>>,
    inputs: Query<&EditableText, With<TrainStopNameInput>>,
    open_container: Res<OpenContainer>,
    mut rename: ResMut<TrainStopRenameState>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(entity_id) = rename.editing else {
        return;
    };
    if open_container.entity_id != Some(entity_id) {
        rename.cancel();
        if let Some(focus) = input_focus.as_deref_mut() {
            focus.clear();
        }
        return;
    }
    let Some(keyboard) = keyboard else {
        return;
    };
    if !keyboard.just_pressed(KeyCode::Enter) && !keyboard.just_pressed(KeyCode::NumpadEnter) {
        return;
    }
    let Some(focused) = input_focus.as_deref().and_then(InputFocus::get) else {
        return;
    };
    let Ok(input) = inputs.get(focused) else {
        return;
    };
    if input.is_composing() {
        return;
    }
    commit_rename(&mut rename, entity_id, editor_value(input), &mut commands);
    if let Some(focus) = input_focus.as_deref_mut() {
        focus.clear();
    }
}

/// Sends the typed name, unless it is blank — a station nobody could name is
/// not a rename, so an empty buffer simply abandons the edit.
fn commit_rename(
    rename: &mut TrainStopRenameState,
    entity_id: EntityId,
    value: String,
    commands: &mut MessageWriter<SimCommandRequest>,
) {
    let name = value.trim().to_string();
    rename.cancel();
    if name.is_empty() {
        return;
    }
    commands.write(SimCommandRequest(SimCommand::RenameTrainStop {
        stop: entity_id,
        name,
    }));
}

pub(crate) fn handle_train_stop_limit_buttons(
    buttons: Query<(&Interaction, &TrainStopLimitStepButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some(delta) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| button.0)
    else {
        return;
    };
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    let Some(current) = sim
        .read()
        .train_stop(entity_id)
        .map(|state| state.train_limit)
    else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    // Clamped at one rather than refused at zero: a stepper that stopped
    // responding at the bottom would look broken, and the simulation refuses a
    // zero limit anyway.
    let train_limit = current.saturating_add_signed(delta).max(1);
    commands.write(SimCommandRequest(SimCommand::SetTrainStopLimit {
        stop: entity_id,
        train_limit,
    }));
}

/// Opens (or closes) the picker for the channel the limit is read from.
pub(crate) fn handle_train_stop_limit_signal_button(
    buttons: Query<&Interaction, (Changed<Interaction>, With<TrainStopLimitSignalButton>)>,
    mut editor: ResMut<CircuitEditorState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !buttons.iter().any(pressed) {
        return;
    }
    sounds.write(SoundEvent::UiClick);
    editor.picker =
        (editor.picker != Some(SignalSlot::TrainStopLimit)).then_some(SignalSlot::TrainStopLimit);
}
