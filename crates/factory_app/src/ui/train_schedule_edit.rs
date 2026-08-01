//! What the schedule editor's buttons do, and the two windows they open.
//!
//! Split from the panel that draws them because the drawing is a function of
//! the schedule and this is a function of a press: keeping them apart is what
//! lets the panel stay a pure view over a snapshot.
//!
//! Two things are picked from a list rather than cycled or typed — the station
//! an entry names, and the channel a condition counts — so there are two picker
//! windows. Neither is an edit on its own: opening one asks a question the
//! player has yet to answer, and nothing is sent until they do.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_data::{FluidId, ItemId, PrototypeCatalog};
use factory_sim::{
    CircuitCondition, Comparator, SignalId, SignalOperand, SimCommand, Simulation, TrainId,
    TrainSchedule, TrainScheduleEntry, TrainWaitCondition, TrainWaitConditionGroup,
    TrainWaitConditionKind,
};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::circuit::state::cycle;
use crate::ui::circuit::widgets::{spawn_button, spawn_caption, spawn_heading};
use crate::ui::resources::OpenContainer;
use crate::ui::signal_picker::{SignalFilter, signal_picker_root, spawn_signal_picker_contents};
use crate::ui::train_schedule_panel::{
    ConditionPart, ConditionRef, ConditionSlot, MILLIUNITS_PER_UNIT, ScheduleAddConditionButton,
    ScheduleAddGroupButton, ScheduleChannelButton, ScheduleComparatorButton,
    ScheduleConditionKindButton, ScheduleConditionRemoveButton, ScheduleConditionStepButton,
    ScheduleManualButton, ScheduleOperandModeButton, ScheduleRemoveButton, ScheduleRevision,
    ScheduleStationButton, WAIT_STEP_TICKS, schedule_snapshot, station_names,
};
use crate::ui::window_sync::{WindowRootQuery, sync_window};

/// What a fresh timed condition starts at: five seconds, long enough to be a
/// real wait and short enough to watch happen.
const DEFAULT_WAIT_TICKS: u64 = 5 * WAIT_STEP_TICKS;

/// Which list the editor is waiting on an answer from.
///
/// One at a time: both windows sit in the same corner, and a press in either
/// fills exactly the slot that opened it.
#[derive(Resource, Default)]
pub(crate) struct TrainScheduleEditorState {
    /// The condition slot the signal grid is filling.
    pub(crate) channel: Option<ConditionSlot>,
    /// The entry a station is being chosen for. An index one past the end
    /// appends, which is how the "Add stop" button asks.
    pub(crate) station: Option<usize>,
    /// What the schedule looked like when the open picker was opened.
    ///
    /// A picker outlives the press that opened it — that is the whole point of
    /// it — so the schedule can move underneath it while the player is reading
    /// the list. Nothing the editor itself does can cause that, because every
    /// edit closes the pickers; the simulation can, and does. Mine the last
    /// stop of a name and `forget_train_stop` drops the entries naming it, and
    /// the addresses held above then name whatever slid down into them.
    revision: Option<ScheduleRevision>,
}

impl TrainScheduleEditorState {
    fn close(&mut self) {
        self.channel = None;
        self.station = None;
        self.revision = None;
    }

    /// The schedule an answer applies to, or `None` if it has moved on since
    /// the question was asked.
    fn answered_schedule(
        &self,
        sim: &Simulation,
        open: &OpenContainer,
    ) -> Option<(TrainId, TrainSchedule)> {
        schedule_for_press(sim, open, self.revision?)
    }
}

/// A station name offered to an entry. `None` backs out without choosing one.
#[derive(Component, Clone, Debug)]
pub(crate) struct StationPickerButton(pub(crate) Option<String>);

/// `None` is the grid's clearing button; `Some` assigns that signal.
#[derive(Component)]
pub(crate) struct ScheduleSignalPickerButton(pub(crate) Option<SignalId>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StationPickerSnapshot {
    entry: usize,
    names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScheduleSignalPickerSnapshot {
    slot: ConditionSlot,
    filter: SignalFilter,
}

fn pressed(interaction: &Interaction) -> bool {
    *interaction == Interaction::Pressed
}

/// The open train's schedule and its id, for an edit to start from.
fn open_schedule(sim: &Simulation, open: &OpenContainer) -> Option<(TrainId, TrainSchedule)> {
    let train_id = sim.rolling_stock_piece(open.rolling_stock?)?.train;
    Some((train_id, sim.train(train_id)?.schedule.clone()))
}

/// The schedule an edit starts from, or `None` when the button that was pressed
/// belongs to a panel the schedule has since outgrown.
///
/// A press is answered at least one frame after the panel that drew it was
/// built, and the panel is rebuilt *after* these handlers run — so an edit that
/// has already reached the simulation leaves a frame in which the buttons on
/// screen still address the schedule as it was. Checking that the address still
/// exists does not catch that: remove the first of two conditions and the
/// second slides into its address, where a second press of the same stale
/// button would remove it as well. Comparing what the panel was drawn from
/// against what it would be drawn from now rejects exactly those presses, and
/// only those.
fn schedule_for_press(
    sim: &Simulation,
    open: &OpenContainer,
    revision: ScheduleRevision,
) -> Option<(TrainId, TrainSchedule)> {
    let current = schedule_snapshot(sim, open.rolling_stock?)?;
    if current.revision() != revision.0 {
        return None;
    }
    open_schedule(sim, open)
}

/// The first press in a query of schedule buttons, with the revision it was
/// drawn under.
fn pressed_schedule_button<B: Component + Copy>(
    buttons: &Query<(&Interaction, &B, &ScheduleRevision), Changed<Interaction>>,
) -> Option<(B, ScheduleRevision)> {
    buttons
        .iter()
        .find(|(interaction, _, _)| pressed(interaction))
        .map(|(_, button, revision)| (*button, *revision))
}

fn send(
    commands: &mut MessageWriter<SimCommandRequest>,
    train_id: TrainId,
    schedule: TrainSchedule,
) {
    commands.write(SimCommandRequest(SimCommand::SetTrainSchedule {
        train_id,
        schedule,
    }));
}

/// Opens the station list for an entry, or for one past the end to append.
pub(crate) fn handle_schedule_station_buttons(
    buttons: Query<(&Interaction, &ScheduleStationButton, &ScheduleRevision), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    if schedule_for_press(&sim.read(), &open_container, revision).is_none() {
        return;
    }
    sounds.write(SoundEvent::UiClick);
    editor.close();
    editor.station = Some(button.0);
    editor.revision = Some(revision);
}

/// Opens the signal grid for one half of a condition.
pub(crate) fn handle_schedule_channel_buttons(
    buttons: Query<(&Interaction, &ScheduleChannelButton, &ScheduleRevision), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    if schedule_for_press(&sim.read(), &open_container, revision).is_none() {
        return;
    }
    sounds.write(SoundEvent::UiClick);
    editor.close();
    editor.channel = Some(button.0);
    editor.revision = Some(revision);
}

/// Hands the train back to its schedule, or takes it off it.
pub(crate) fn handle_schedule_manual_buttons(
    buttons: Query<(&Interaction, &ScheduleManualButton, &ScheduleRevision), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, _)) = schedule_for_press(&sim, &open_container, revision) else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    commands.write(SimCommandRequest(SimCommand::SetTrainManual {
        train_id,
        manual: !button.0,
    }));
}

pub(crate) fn handle_schedule_remove_buttons(
    buttons: Query<(&Interaction, &ScheduleRemoveButton, &ScheduleRevision), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = schedule_for_press(&sim, &open_container, revision) else {
        return;
    };
    if schedule.remove_entry(button.0).is_none() {
        return;
    }
    sounds.write(SoundEvent::UiClick);
    // Whatever the pickers were asking about was addressed against the list as
    // it was a moment ago; an answer given now would fill the wrong row.
    editor.close();
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_add_group_buttons(
    buttons: Query<
        (&Interaction, &ScheduleAddGroupButton, &ScheduleRevision),
        Changed<Interaction>,
    >,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = schedule_for_press(&sim, &open_container, revision) else {
        return;
    };
    let Some(entry) = schedule.entries.get_mut(button.0) else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    entry
        .wait_conditions
        .push(TrainWaitConditionGroup(vec![default_condition()]));
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_add_condition_buttons(
    buttons: Query<
        (&Interaction, &ScheduleAddConditionButton, &ScheduleRevision),
        Changed<Interaction>,
    >,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = schedule_for_press(&sim, &open_container, revision) else {
        return;
    };
    let Some(alternative) = schedule
        .entries
        .get_mut(button.entry)
        .and_then(|entry| entry.wait_conditions.get_mut(button.group))
    else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    alternative.0.push(default_condition());
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_condition_remove_buttons(
    buttons: Query<
        (
            &Interaction,
            &ScheduleConditionRemoveButton,
            &ScheduleRevision,
        ),
        Changed<Interaction>,
    >,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((button, revision)) = pressed_schedule_button(&buttons) else {
        return;
    };
    let address = button.0;
    let sim = sim.read();
    let Some((train_id, mut schedule)) = schedule_for_press(&sim, &open_container, revision) else {
        return;
    };
    let Some(entry) = schedule.entries.get_mut(address.entry) else {
        return;
    };
    if entry.condition(address.group, address.index).is_none() {
        return;
    }
    sounds.write(SoundEvent::UiClick);
    entry.remove_condition(address.group, address.index);
    editor.close();
    send(&mut commands, train_id, schedule);
}

/// Steps a condition's kind, its comparator, its operand mode, or its number —
/// four buttons, one shape, because each is a rewrite of one condition in
/// place.
pub(crate) fn handle_schedule_condition_edit_buttons(
    buttons: ConditionEditButtons,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    // Shift steps a cycling button backwards, the rule every other cycling
    // button in the game follows.
    let backwards = keyboard
        .as_deref()
        .is_some_and(|keys| keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight));
    let Some((address, edit, revision)) = buttons.pressed_edit() else {
        return;
    };

    let sim = sim.read();
    let Some((train_id, mut schedule)) = schedule_for_press(&sim, &open_container, revision) else {
        return;
    };
    let catalog = sim.catalog();
    let Some(entry) = schedule.entries.get_mut(address.entry) else {
        return;
    };
    let Some(current) = entry.condition(address.group, address.index) else {
        return;
    };
    let replacement = match edit {
        Edit::Kind => cycled_condition(current, backwards, catalog),
        Edit::Comparator => current.with_comparator(cycle(
            &Comparator::ALL,
            current.comparator().unwrap_or(Comparator::Greater),
            backwards,
        )),
        Edit::OperandMode => toggled_operand_mode(current),
        Edit::Step(delta) => stepped_condition(current, delta),
    };
    sounds.write(SoundEvent::UiClick);
    entry.set_condition(address.group, address.index, replacement);
    send(&mut commands, train_id, schedule);
}

/// Which of the four in-place rewrites a press asked for.
#[derive(Clone, Copy)]
enum Edit {
    Kind,
    Comparator,
    OperandMode,
    Step(i32),
}

/// The four buttons that rewrite one condition where it stands.
///
/// One parameter rather than four, and one system rather than four, because
/// they all end in the same place: read the schedule, replace one condition,
/// send the whole thing. Two of them acting on one frame would each answer
/// against the same pre-edit schedule, and the second would land on top of the
/// first as though it had never happened.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ConditionEditButtons<'w, 's> {
    kinds: Query<
        'w,
        's,
        (
            &'static Interaction,
            &'static ScheduleConditionKindButton,
            &'static ScheduleRevision,
        ),
        Changed<Interaction>,
    >,
    comparators: Query<
        'w,
        's,
        (
            &'static Interaction,
            &'static ScheduleComparatorButton,
            &'static ScheduleRevision,
        ),
        Changed<Interaction>,
    >,
    modes: Query<
        'w,
        's,
        (
            &'static Interaction,
            &'static ScheduleOperandModeButton,
            &'static ScheduleRevision,
        ),
        Changed<Interaction>,
    >,
    steps: Query<
        'w,
        's,
        (
            &'static Interaction,
            &'static ScheduleConditionStepButton,
            &'static ScheduleRevision,
        ),
        Changed<Interaction>,
    >,
}

impl ConditionEditButtons<'_, '_> {
    /// The one press to act on this frame, if any, with the revision it was
    /// drawn under.
    fn pressed_edit(&self) -> Option<(ConditionRef, Edit, ScheduleRevision)> {
        pressed_schedule_button(&self.kinds)
            .map(|(button, revision)| (button.0, Edit::Kind, revision))
            .or_else(|| {
                pressed_schedule_button(&self.comparators)
                    .map(|(button, revision)| (button.0, Edit::Comparator, revision))
            })
            .or_else(|| {
                pressed_schedule_button(&self.modes)
                    .map(|(button, revision)| (button.0, Edit::OperandMode, revision))
            })
            .or_else(|| {
                pressed_schedule_button(&self.steps).map(|(button, revision)| {
                    (button.condition, Edit::Step(button.delta), revision)
                })
            })
    }
}

/// The condition a fresh row starts as: a full load, which needs nothing
/// configured and is what most stops actually want.
const fn default_condition() -> TrainWaitCondition {
    TrainWaitCondition::CargoFull
}

/// The next kind round the cycle that this catalog can actually express.
///
/// A kind that names a channel needs one to name. A catalog with no fluids in
/// it cannot make a fluid condition, and a button that does nothing when
/// pressed is worse than one that is not offered — so such a kind is stepped
/// over rather than offered and then refused.
fn cycled_condition(
    current: TrainWaitCondition,
    backwards: bool,
    catalog: &PrototypeCatalog,
) -> TrainWaitCondition {
    let kinds = TrainWaitConditionKind::ALL;
    let start = kinds
        .iter()
        .position(|kind| *kind == current.kind())
        .unwrap_or(0);
    for step in 1..=kinds.len() {
        let offset = if backwards { kinds.len() - step } else { step };
        let kind = kinds[(start + offset) % kinds.len()];
        if let Some(condition) = condition_of_kind(kind, current, catalog) {
            return condition;
        }
    }
    current
}

/// A condition of `kind`, carrying over from `previous` whatever the new kind
/// can still use — which is the comparator, and nothing else: an item count and
/// a fluid count are not the same number, and carrying one into the other would
/// silently ask for a hundred thousand units of water.
fn condition_of_kind(
    kind: TrainWaitConditionKind,
    previous: TrainWaitCondition,
    catalog: &PrototypeCatalog,
) -> Option<TrainWaitCondition> {
    let comparator = previous.comparator().unwrap_or(Comparator::Greater);
    Some(match kind {
        TrainWaitConditionKind::CargoFull => TrainWaitCondition::CargoFull,
        TrainWaitConditionKind::CargoEmpty => TrainWaitCondition::CargoEmpty,
        TrainWaitConditionKind::TimePassed => TrainWaitCondition::TimePassed {
            ticks: DEFAULT_WAIT_TICKS,
        },
        TrainWaitConditionKind::Inactivity => TrainWaitCondition::Inactivity {
            ticks: DEFAULT_WAIT_TICKS,
        },
        TrainWaitConditionKind::ItemCount => TrainWaitCondition::ItemCount {
            item: first_item(catalog)?,
            comparator,
            count: 0,
        },
        TrainWaitConditionKind::FluidCount => TrainWaitCondition::FluidCount {
            fluid: first_fluid(catalog)?,
            comparator,
            milliunits: 0,
        },
        TrainWaitConditionKind::Circuit => TrainWaitCondition::Circuit(CircuitCondition {
            left: first_signal(catalog)?,
            comparator,
            right: SignalOperand::Constant(0),
        }),
    })
}

fn first_item(catalog: &PrototypeCatalog) -> Option<ItemId> {
    catalog.items.first().map(|item| item.id)
}

fn first_fluid(catalog: &PrototypeCatalog) -> Option<FluidId> {
    catalog.fluids.first().map(|fluid| fluid.id)
}

/// A channel a circuit condition can start on.
///
/// The first *concrete* virtual signal, not simply the first one: the catalog
/// leads with `each`, `anything`, and `everything`, and a wait condition naming
/// one of those is refused outright by `set_train_schedule`. Starting there
/// would make the circuit kind impossible to reach — cycling onto it would send
/// a schedule that comes straight back rejected. A catalog with nothing but
/// wildcards falls through to the first item, because every catalog has items.
fn first_signal(catalog: &PrototypeCatalog) -> Option<SignalId> {
    catalog
        .virtual_signals
        .iter()
        .find(|signal| !signal.kind.is_wildcard())
        .map(|signal| SignalId::Virtual(signal.id))
        .or_else(|| first_item(catalog).map(SignalId::Item))
}

/// Flips a circuit condition's right-hand operand between a signal and a plain
/// number, switching to the channel the condition is already about — so the
/// toggle reads as "against this channel" rather than as losing what was
/// configured.
fn toggled_operand_mode(condition: TrainWaitCondition) -> TrainWaitCondition {
    let TrainWaitCondition::Circuit(inner) = condition else {
        return condition;
    };
    TrainWaitCondition::Circuit(CircuitCondition {
        right: match inner.right {
            SignalOperand::Constant(_) => SignalOperand::Signal(inner.left),
            SignalOperand::Signal(_) => SignalOperand::Constant(0),
        },
        ..inner
    })
}

/// The condition with its number moved by one step.
///
/// What a step *is* depends on the kind, and that is the point of doing it
/// here: a second for the timed conditions, an item for an item count, a whole
/// unit for a fluid. One stepper that moved every one of them by one would be
/// useless for two of the three.
fn stepped_condition(condition: TrainWaitCondition, delta: i32) -> TrainWaitCondition {
    let step_ticks = |ticks: u64| {
        let magnitude = u64::from(delta.unsigned_abs()).saturating_mul(WAIT_STEP_TICKS);
        if delta < 0 {
            ticks.saturating_sub(magnitude)
        } else {
            ticks.saturating_add(magnitude)
        }
    };
    match condition {
        TrainWaitCondition::TimePassed { ticks } => TrainWaitCondition::TimePassed {
            ticks: step_ticks(ticks),
        },
        TrainWaitCondition::Inactivity { ticks } => TrainWaitCondition::Inactivity {
            ticks: step_ticks(ticks),
        },
        TrainWaitCondition::ItemCount {
            item,
            comparator,
            count,
        } => TrainWaitCondition::ItemCount {
            item,
            comparator,
            count: count.saturating_add(delta).max(0),
        },
        TrainWaitCondition::FluidCount {
            fluid,
            comparator,
            milliunits,
        } => TrainWaitCondition::FluidCount {
            fluid,
            comparator,
            milliunits: milliunits
                .saturating_add(delta.saturating_mul(MILLIUNITS_PER_UNIT))
                .max(0),
        },
        TrainWaitCondition::Circuit(inner) => TrainWaitCondition::Circuit(CircuitCondition {
            right: match inner.right {
                SignalOperand::Constant(value) => {
                    SignalOperand::Constant(value.saturating_add(delta))
                }
                signal @ SignalOperand::Signal(_) => signal,
            },
            ..inner
        }),
        other => other,
    }
}

/// The signals a condition's slot will accept.
///
/// Never `Any`: every signal in a wait condition is read as one number off the
/// network the train is standing at, and `set_train_schedule` refuses a
/// combinator wildcard on either side of the comparison. Offering one would be
/// offering a pick that comes back rejected.
fn slot_filter(condition: TrainWaitCondition, part: ConditionPart) -> SignalFilter {
    match (part, condition.kind()) {
        (ConditionPart::Subject, TrainWaitConditionKind::ItemCount) => SignalFilter::ItemsOnly,
        (ConditionPart::Subject, TrainWaitConditionKind::FluidCount) => SignalFilter::FluidsOnly,
        _ => SignalFilter::ValuesOnly,
    }
}

/// The condition with a picked signal written into one of its halves, or `None`
/// when the pick cannot land there.
///
/// A `None` signal is the grid's clearing button: on a circuit condition's
/// right-hand operand that means "back to a plain number", and on anything else
/// there is nothing sensible to clear a condition's subject to, so the pick is
/// refused rather than leaving a condition that names nothing.
fn condition_with_signal(
    condition: TrainWaitCondition,
    part: ConditionPart,
    signal: Option<SignalId>,
) -> Option<TrainWaitCondition> {
    match (part, condition, signal) {
        (
            ConditionPart::Subject,
            TrainWaitCondition::ItemCount {
                comparator, count, ..
            },
            Some(SignalId::Item(item)),
        ) => Some(TrainWaitCondition::ItemCount {
            item,
            comparator,
            count,
        }),
        (
            ConditionPart::Subject,
            TrainWaitCondition::FluidCount {
                comparator,
                milliunits,
                ..
            },
            Some(SignalId::Fluid(fluid)),
        ) => Some(TrainWaitCondition::FluidCount {
            fluid,
            comparator,
            milliunits,
        }),
        (ConditionPart::Subject, TrainWaitCondition::Circuit(inner), Some(left)) => {
            Some(TrainWaitCondition::Circuit(CircuitCondition {
                left,
                ..inner
            }))
        }
        (ConditionPart::CircuitRight, TrainWaitCondition::Circuit(inner), signal) => {
            Some(TrainWaitCondition::Circuit(CircuitCondition {
                right: match signal {
                    Some(signal) => SignalOperand::Signal(signal),
                    None => SignalOperand::Constant(0),
                },
                ..inner
            }))
        }
        _ => None,
    }
}

/// The condition an open channel picker is filling, if it still exists.
fn slot_condition(
    sim: &Simulation,
    open: &OpenContainer,
    slot: ConditionSlot,
) -> Option<TrainWaitCondition> {
    let (_, schedule) = open_schedule(sim, open)?;
    schedule
        .entries
        .get(slot.condition.entry)?
        .condition(slot.condition.group, slot.condition.index)
}

pub(crate) fn sync_station_picker(
    mut commands: Commands,
    sim: Res<SimResource>,
    editor: Res<TrainScheduleEditorState>,
    open_container: Res<OpenContainer>,
    mut roots: WindowRootQuery<StationPickerSnapshot>,
) {
    // The picker belongs to the open train, so it closes with the window.
    let Some(entry) = open_container.rolling_stock.and(editor.station) else {
        for (entity, _, _) in roots.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let sim = sim.read();
    sync_window(
        &mut commands,
        &mut roots,
        true,
        true,
        || StationPickerSnapshot {
            entry,
            names: station_names(&sim),
        },
        station_picker_root,
        |root, snapshot| {
            spawn_heading(
                root,
                &format!("Stop {} — pick a station", snapshot.entry + 1),
            );
            if snapshot.names.is_empty() {
                spawn_caption(root, "No stations yet. Build a train stop first.");
            }
            // The names scroll and the chrome around them does not, so a railway
            // with more stations than fit on screen still leaves "Cancel" where
            // the player can reach it. `ScrollArea` rather than `Overflow`
            // alone: the overflow only clips, and the component is what carries
            // the scroll position and the wheel observer.
            root.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    overflow: Overflow::scroll_y(),
                    scrollbar_width: 10.0,
                    ..default()
                },
                BackgroundColor(Color::NONE),
                ScrollArea,
            ))
            .with_children(|list| {
                for name in &snapshot.names {
                    spawn_button(
                        list,
                        200.0,
                        name,
                        StationPickerButton(Some(name.clone())),
                        (),
                    );
                }
            });
            spawn_button(root, 80.0, "Cancel", StationPickerButton(None), ());
        },
    );
}

fn station_picker_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Px(240.0),
            max_height: Val::Vh(50.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.98)),
        GlobalZIndex(1200),
    )
}

pub(crate) fn sync_schedule_signal_picker(
    mut commands: Commands,
    sim: Res<SimResource>,
    editor: Res<TrainScheduleEditorState>,
    open_container: Res<OpenContainer>,
    mut roots: WindowRootQuery<ScheduleSignalPickerSnapshot>,
) {
    let sim = sim.read();
    // A slot naming a condition that has since been removed has nothing to
    // fill, so the window goes with it.
    let open = editor
        .channel
        .filter(|_| open_container.rolling_stock.is_some())
        .and_then(|slot| {
            slot_condition(&sim, &open_container, slot)
                .map(|condition| (slot, slot_filter(condition, slot.part)))
        });
    let Some((slot, filter)) = open else {
        for (entity, _, _) in roots.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };
    sync_window(
        &mut commands,
        &mut roots,
        true,
        true,
        || ScheduleSignalPickerSnapshot { slot, filter },
        signal_picker_root,
        |root, snapshot| {
            spawn_signal_picker_contents(
                root,
                sim.catalog(),
                snapshot.filter,
                // Only an operand can go back to being a plain number; a
                // condition's subject has nothing to clear to.
                if snapshot.slot.part == ConditionPart::CircuitRight {
                    "Use a number"
                } else {
                    "Cancel"
                },
                ScheduleSignalPickerButton,
            );
        },
    );
}

/// Points an entry at the station the player chose, appending a stop when the
/// picker was opened one past the end.
pub(crate) fn handle_station_picker_buttons(
    buttons: Query<(&Interaction, &StationPickerButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some(name) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| button.0.clone())
    else {
        return;
    };
    let Some(entry) = editor.station else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    let sim = sim.read();
    let answered = editor.answered_schedule(&sim, &open_container);
    editor.close();
    let Some(name) = name else {
        return;
    };
    let Some((train_id, mut schedule)) = answered else {
        return;
    };
    if let Some(existing) = schedule.entries.get_mut(entry) {
        existing.stop_name = name;
    } else if entry == schedule.entries.len() {
        // Only the exact append index appends. A stale one names an entry that
        // has since been removed, and adding a stop nobody asked for is worse
        // than doing nothing.
        schedule.insert_entry(entry, TrainScheduleEntry::new(name));
    } else {
        return;
    }
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_signal_picker_buttons(
    buttons: Query<(&Interaction, &ScheduleSignalPickerButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some(signal) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| button.0)
    else {
        return;
    };
    let Some(slot) = editor.channel else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    let sim = sim.read();
    let answered = editor.answered_schedule(&sim, &open_container);
    editor.close();
    let Some((train_id, mut schedule)) = answered else {
        return;
    };
    let Some(entry) = schedule.entries.get_mut(slot.condition.entry) else {
        return;
    };
    let Some(current) = entry.condition(slot.condition.group, slot.condition.index) else {
        return;
    };
    let Some(replacement) = condition_with_signal(current, slot.part, signal) else {
        return;
    };
    entry.set_condition(slot.condition.group, slot.condition.index, replacement);
    send(&mut commands, train_id, schedule);
}

/// Closes both pickers when the window they belong to does, so a picker never
/// outlives the schedule it was filling.
pub(crate) fn close_schedule_pickers_with_window(
    open_container: Res<OpenContainer>,
    mut editor: ResMut<TrainScheduleEditorState>,
) {
    if open_container.rolling_stock.is_none()
        && (editor.channel.is_some() || editor.station.is_some())
    {
        editor.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> PrototypeCatalog {
        Simulation::new_test_world(1).catalog().clone()
    }

    /// Cycling a kind walks every kind the catalog can express and comes back
    /// round, which is what makes the item, fluid, and circuit conditions —
    /// the ones the old editor could not write at all — reachable.
    #[test]
    fn cycling_a_kind_reaches_every_condition_the_catalog_can_express() {
        let catalog = catalog();
        let mut condition = default_condition();
        let mut seen = vec![condition.kind()];
        for _ in 0..TrainWaitConditionKind::ALL.len() {
            condition = cycled_condition(condition, false, &catalog);
            seen.push(condition.kind());
        }
        assert_eq!(seen.first(), seen.last(), "the cycle comes back round");
        for kind in TrainWaitConditionKind::ALL {
            assert!(seen.contains(&kind), "{kind:?} is never offered: {seen:?}");
        }
    }

    /// A circuit condition has to start on a channel the simulation will
    /// accept. The catalog leads with `each`, `anything`, and `everything`, and
    /// `set_train_schedule` refuses all three — so starting there would make the
    /// circuit kind impossible to reach at all: cycling onto it would send a
    /// schedule that came straight back rejected.
    #[test]
    fn a_circuit_condition_starts_on_a_channel_the_simulation_accepts() {
        let mut sim = Simulation::new_test_world(1);
        let catalog = sim.catalog().clone();
        assert!(
            catalog
                .virtual_signals
                .first()
                .is_some_and(|signal| signal.kind.is_wildcard()),
            "this test is only meaningful while the catalog still leads with a wildcard"
        );

        let condition = condition_of_kind(
            TrainWaitConditionKind::Circuit,
            default_condition(),
            &catalog,
        )
        .expect("the catalog can express a circuit condition");
        let train_id = sim
            .rolling_stock()
            .next()
            .map(|stock| stock.train)
            .unwrap_or(TrainId::new(1));
        // The proof is the simulation taking it, not the shape of the signal.
        let schedule = TrainSchedule {
            entries: vec![TrainScheduleEntry {
                stop_name: "Depot".into(),
                wait_conditions: vec![TrainWaitConditionGroup(vec![condition])],
            }],
            current: 0,
        };
        assert!(
            !matches!(
                sim.set_train_schedule(train_id, schedule),
                Err(factory_sim::TrainControlError::WildcardSignal(_))
            ),
            "the condition the editor builds must not be one the simulation refuses"
        );
    }

    /// The picker must not offer what the slot cannot hold, for the same
    /// reason: a wildcard picked into a wait condition comes back rejected.
    #[test]
    fn a_condition_slot_never_offers_a_wildcard() {
        let catalog = catalog();
        let circuit = condition_of_kind(
            TrainWaitConditionKind::Circuit,
            default_condition(),
            &catalog,
        )
        .expect("the catalog can express a circuit condition");
        for part in [ConditionPart::Subject, ConditionPart::CircuitRight] {
            assert_eq!(
                slot_filter(circuit, part),
                SignalFilter::ValuesOnly,
                "both halves of a circuit condition are read as values"
            );
        }
    }

    /// A comparator change keeps what is being compared, and a kind that
    /// compares nothing is left alone rather than gaining one it never reads.
    #[test]
    fn changing_a_comparator_keeps_the_channel_under_it() {
        let catalog = catalog();
        let item = TrainWaitCondition::ItemCount {
            item: first_item(&catalog).expect("a catalog has items"),
            comparator: Comparator::Greater,
            count: 400,
        };
        let changed = item.with_comparator(Comparator::Less);
        assert_eq!(changed.comparator(), Some(Comparator::Less));
        assert_eq!(changed.kind(), TrainWaitConditionKind::ItemCount);

        assert_eq!(TrainWaitCondition::CargoFull.comparator(), None);
    }

    /// Each kind's step is the unit a player thinks in for that kind, and none
    /// of them can be driven below zero.
    #[test]
    fn a_step_means_what_the_kind_under_it_means() {
        assert_eq!(
            stepped_condition(TrainWaitCondition::TimePassed { ticks: 60 }, 1),
            TrainWaitCondition::TimePassed { ticks: 120 }
        );
        assert_eq!(
            stepped_condition(TrainWaitCondition::TimePassed { ticks: 60 }, -5),
            TrainWaitCondition::TimePassed { ticks: 0 },
            "a wait cannot be stepped to a negative length"
        );

        let catalog = catalog();
        let fluid = TrainWaitCondition::FluidCount {
            fluid: first_fluid(&catalog).expect("a catalog has fluids"),
            comparator: Comparator::Greater,
            milliunits: 0,
        };
        let TrainWaitCondition::FluidCount { milliunits, .. } = stepped_condition(fluid, 1) else {
            panic!("stepping a fluid condition keeps it one");
        };
        assert_eq!(milliunits, MILLIUNITS_PER_UNIT, "a fluid steps by the unit");
    }

    /// The right-hand operand flips to the channel the condition is already
    /// about, so the toggle is a way of saying "against this" rather than a way
    /// of losing what was configured.
    #[test]
    fn toggling_an_operand_keeps_the_channel_it_was_about() {
        let catalog = catalog();
        let left = first_signal(&catalog).expect("a catalog has signals");
        let condition = TrainWaitCondition::Circuit(CircuitCondition {
            left,
            comparator: Comparator::Greater,
            right: SignalOperand::Constant(0),
        });
        let TrainWaitCondition::Circuit(inner) = toggled_operand_mode(condition) else {
            panic!("it is still a circuit condition");
        };
        assert_eq!(inner.right, SignalOperand::Signal(left));
        assert_eq!(inner.left, left, "the left-hand channel is untouched");
    }

    /// A slot only accepts what its kind can hold, so the grid cannot put a
    /// fluid into an item count.
    #[test]
    fn a_pick_only_lands_where_the_kind_can_hold_it() {
        let catalog = catalog();
        let item_id = first_item(&catalog).expect("a catalog has items");
        let fluid_id = first_fluid(&catalog).expect("a catalog has fluids");
        let item = TrainWaitCondition::ItemCount {
            item: item_id,
            comparator: Comparator::Greater,
            count: 0,
        };
        assert_eq!(
            slot_filter(item, ConditionPart::Subject),
            SignalFilter::ItemsOnly
        );
        assert!(
            condition_with_signal(
                item,
                ConditionPart::Subject,
                Some(SignalId::Fluid(fluid_id))
            )
            .is_none(),
            "a fluid is not an item count's subject"
        );
        assert!(
            condition_with_signal(item, ConditionPart::Subject, Some(SignalId::Item(item_id)))
                .is_some()
        );
        assert!(
            condition_with_signal(item, ConditionPart::Subject, None).is_none(),
            "there is nothing to clear a subject to"
        );
    }
}
