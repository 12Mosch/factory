//! The schedule editor: the ordered list of stations a train serves and what
//! keeps it at each of them.
//!
//! Shown inside the rolling-stock window, because a schedule belongs to the
//! *train* rather than to any one piece of it: opening any wagon of a train
//! shows the same list, and editing it from a wagon is editing the train.
//!
//! Two decisions shape the controls.
//!
//! * **Stations are chosen, not typed.** An entry names a station by name, and
//!   a name that answers to nothing is a train with nowhere to go, so the entry
//!   cycles through the names that exist rather than offering a text field that
//!   could hold anything. Renaming a station is done at the station, where the
//!   consequences of it are visible.
//! * **One condition per entry here.** The simulation stores ORs of ANDs, and
//!   saves and loads the whole of that; what this edits is the common case —
//!   one rule per stop — so a row stays a row. An entry carrying something
//!   richer is shown as what it is and left alone rather than flattened by an
//!   editor that cannot express it.

use bevy::prelude::*;
use factory_sim::{
    RollingStockId, SignalOperand, SimCommand, Simulation, TrainId, TrainSchedule,
    TrainScheduleEntry, TrainWaitCondition, TrainWaitConditionGroup,
};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::circuit::signals::signal_display_name;
use crate::ui::circuit::state::cycle;
use crate::ui::circuit::widgets::{
    LABEL_COLOR, spawn_button, spawn_caption, spawn_heading, spawn_row,
};
use crate::ui::resources::OpenContainer;

/// Ticks a step button moves a timed condition by: one second, the unit a
/// player thinks about a station wait in.
const WAIT_STEP_TICKS: u64 = 60;

/// What a fresh timed condition starts at: five seconds, long enough to be a
/// real wait and short enough to watch happen.
const DEFAULT_WAIT_TICKS: u64 = 5 * WAIT_STEP_TICKS;

/// The condition kinds the row cycles through.
///
/// A closed list rather than the whole of [`TrainWaitCondition`]: item, fluid,
/// and circuit conditions each need a channel picked as well as a comparator,
/// which is more than a cycling button can say. They are readable here and
/// editable where the signals they compare against are — see the module note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaitKind {
    Immediate,
    TimePassed,
    Inactivity,
    CargoFull,
    CargoEmpty,
}

impl WaitKind {
    const ALL: [Self; 5] = [
        Self::Immediate,
        Self::TimePassed,
        Self::Inactivity,
        Self::CargoFull,
        Self::CargoEmpty,
    ];

    /// The condition this kind produces, keeping whatever tick count the entry
    /// already carried so cycling past a timed condition and back does not
    /// silently reset it.
    fn condition(self, ticks: u64) -> Option<TrainWaitCondition> {
        match self {
            Self::Immediate => None,
            Self::TimePassed => Some(TrainWaitCondition::TimePassed { ticks }),
            Self::Inactivity => Some(TrainWaitCondition::Inactivity { ticks }),
            Self::CargoFull => Some(TrainWaitCondition::CargoFull),
            Self::CargoEmpty => Some(TrainWaitCondition::CargoEmpty),
        }
    }
}

/// A row of the editor as it was built: what the entry says, so the window
/// rebuilds when an entry is added, removed, or changed and stays put while the
/// train runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScheduleRowSnapshot {
    pub(crate) stop_name: String,
    pub(crate) condition: String,
    /// Whether the row's condition is one the stepper can move.
    pub(crate) timed: bool,
}

/// What the schedule editor was built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScheduleSnapshot {
    pub(crate) rows: Vec<ScheduleRowSnapshot>,
    /// Whether any station exists to name. With none, the editor says so
    /// instead of offering an "add" button that could only add nothing.
    pub(crate) has_stations: bool,
}

#[derive(Component)]
pub(crate) struct ScheduleStatusText;

#[derive(Component)]
pub(crate) struct ScheduleStopCycleButton {
    pub(crate) index: usize,
    pub(crate) backwards: bool,
}

#[derive(Component)]
pub(crate) struct ScheduleConditionCycleButton {
    pub(crate) index: usize,
    pub(crate) backwards: bool,
}

#[derive(Component)]
pub(crate) struct ScheduleWaitStepButton {
    pub(crate) index: usize,
    pub(crate) delta: i64,
}

#[derive(Component)]
pub(crate) struct ScheduleRemoveButton(pub(crate) usize);

#[derive(Component)]
pub(crate) struct ScheduleAddButton;

/// What the editor shows for a train, or `None` for a piece of stock that is
/// not part of one.
pub(crate) fn schedule_snapshot(
    sim: &Simulation,
    stock_id: RollingStockId,
) -> Option<ScheduleSnapshot> {
    let train_id = sim.rolling_stock_piece(stock_id)?.train;
    let train = sim.train(train_id)?;
    Some(ScheduleSnapshot {
        rows: train
            .schedule
            .entries
            .iter()
            .map(|entry| ScheduleRowSnapshot {
                stop_name: entry.stop_name.clone(),
                condition: condition_label(sim, entry),
                timed: matches!(
                    sole_condition(entry),
                    Some(
                        TrainWaitCondition::TimePassed { .. }
                            | TrainWaitCondition::Inactivity { .. }
                    )
                ),
            })
            .collect(),
        has_stations: !station_names(sim).is_empty(),
    })
}

pub(crate) fn spawn_train_schedule_panel(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &ScheduleSnapshot,
) {
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
            spawn_heading(panel, "Schedule");
            for (index, row) in snapshot.rows.iter().enumerate() {
                spawn_row(panel, |controls| {
                    spawn_button(
                        controls,
                        18.0,
                        "<",
                        ScheduleStopCycleButton {
                            index,
                            backwards: true,
                        },
                        (),
                    );
                    controls.spawn((
                        Node {
                            width: Val::Px(120.0),
                            ..default()
                        },
                        Text::new(row.stop_name.clone()),
                        TextFont::from_font_size(11.0),
                        TextColor(Color::WHITE),
                    ));
                    spawn_button(
                        controls,
                        18.0,
                        ">",
                        ScheduleStopCycleButton {
                            index,
                            backwards: false,
                        },
                        (),
                    );
                    spawn_button(controls, 18.0, "x", ScheduleRemoveButton(index), ());
                });
                spawn_row(panel, |controls| {
                    spawn_button(
                        controls,
                        18.0,
                        "<",
                        ScheduleConditionCycleButton {
                            index,
                            backwards: true,
                        },
                        (),
                    );
                    controls.spawn((
                        Node {
                            width: Val::Px(120.0),
                            ..default()
                        },
                        Text::new(row.condition.clone()),
                        TextFont::from_font_size(10.0),
                        TextColor(LABEL_COLOR),
                    ));
                    spawn_button(
                        controls,
                        18.0,
                        ">",
                        ScheduleConditionCycleButton {
                            index,
                            backwards: false,
                        },
                        (),
                    );
                    // A stepper only where there is a number to step; a row
                    // whose condition carries none would answer nothing.
                    if row.timed {
                        spawn_button(
                            controls,
                            18.0,
                            "-",
                            ScheduleWaitStepButton {
                                index,
                                delta: -(WAIT_STEP_TICKS as i64),
                            },
                            (),
                        );
                        spawn_button(
                            controls,
                            18.0,
                            "+",
                            ScheduleWaitStepButton {
                                index,
                                delta: WAIT_STEP_TICKS as i64,
                            },
                            (),
                        );
                    }
                });
            }
            if snapshot.has_stations {
                spawn_row(panel, |controls| {
                    spawn_button(controls, 66.0, "Add stop", ScheduleAddButton, ());
                });
            } else {
                spawn_caption(panel, "Build a train stop to schedule this train");
            }
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                ScheduleStatusText,
            ));
        });
}

/// Writes the one line that changes while the train runs: which entry it is
/// serving and what it is doing about it.
pub(crate) fn update_train_schedule_status(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: Query<&mut Text, With<ScheduleStatusText>>,
) {
    if texts.is_empty() {
        return;
    }
    let sim = sim.read();
    let status = open_container
        .rolling_stock
        .and_then(|stock_id| {
            Some(schedule_status(
                &sim,
                sim.rolling_stock_piece(stock_id)?.train,
            ))
        })
        .unwrap_or_default();
    for mut text in &mut texts {
        if text.0 != status {
            text.0.clone_from(&status);
        }
    }
}

fn schedule_status(sim: &Simulation, train_id: TrainId) -> String {
    let Some(train) = sim.train(train_id) else {
        return String::new();
    };
    let Some(entry) = train.schedule.current_entry() else {
        return "No schedule: this train is driven by hand".to_string();
    };
    let position = format!(
        "{} of {}: {}",
        train.schedule.current + 1,
        train.schedule.entries.len(),
        entry.stop_name
    );
    match (train.scheduled_stop, train.is_waiting_at_scheduled_stop()) {
        (Some(_), true) => format!("{position} — waiting here"),
        (Some(_), false) => format!("{position} — on the way"),
        // A claimed nothing is a station either full or with no track beside
        // it, and both look the same to the player: the train is not moving and
        // it is worth saying why.
        (None, _) => format!("{position} — no platform free"),
    }
}

/// Every station name in the world, in the order the stops were built, without
/// repeats: two platforms of one station are one choice, not two.
///
/// First occurrence wins rather than sorting, so the list a player cycles
/// through does not reorder itself when a station is renamed.
fn station_names(sim: &Simulation) -> Vec<String> {
    let mut names = Vec::new();
    for (_, state) in sim.train_stops() {
        if !names.contains(&state.name) {
            names.push(state.name.clone());
        }
    }
    names
}

/// The one condition a row edits, or `None` for an entry that departs at once.
///
/// Deliberately `None` for anything richer than one condition in one group as
/// well: the editor treats what it cannot express as "leave it alone", which is
/// what `condition_label` says out loud.
fn sole_condition(entry: &TrainScheduleEntry) -> Option<TrainWaitCondition> {
    match entry.wait_conditions.as_slice() {
        [group] => match group.0.as_slice() {
            [condition] => Some(*condition),
            _ => None,
        },
        _ => None,
    }
}

fn condition_label(sim: &Simulation, entry: &TrainScheduleEntry) -> String {
    if entry.wait_conditions.is_empty() {
        return "Leave at once".to_string();
    }
    let Some(condition) = sole_condition(entry) else {
        return "Several conditions".to_string();
    };
    match condition {
        TrainWaitCondition::TimePassed { ticks } => format!("Wait {}", seconds(ticks)),
        TrainWaitCondition::Inactivity { ticks } => format!("Idle {}", seconds(ticks)),
        TrainWaitCondition::CargoFull => "Cargo full".to_string(),
        TrainWaitCondition::CargoEmpty => "Cargo empty".to_string(),
        TrainWaitCondition::ItemCount {
            item,
            comparator,
            count,
        } => format!(
            "{} {} {count}",
            signal_display_name(sim.catalog(), factory_sim::SignalId::Item(item)),
            comparator.symbol()
        ),
        TrainWaitCondition::FluidCount {
            fluid,
            comparator,
            milliunits,
        } => format!(
            "{} {} {}",
            signal_display_name(sim.catalog(), factory_sim::SignalId::Fluid(fluid)),
            comparator.symbol(),
            milliunits / 1_000
        ),
        TrainWaitCondition::Circuit(condition) => format!(
            "{} {} {}",
            signal_display_name(sim.catalog(), condition.left),
            condition.comparator.symbol(),
            match condition.right {
                SignalOperand::Constant(value) => value.to_string(),
                SignalOperand::Signal(signal) => signal_display_name(sim.catalog(), signal),
            }
        ),
    }
}

fn seconds(ticks: u64) -> String {
    if ticks == u64::MAX {
        return "for ever".to_string();
    }
    format!("{}s", ticks / WAIT_STEP_TICKS)
}

/// Which kind a row's condition is, so cycling starts from what is there.
fn wait_kind(entry: &TrainScheduleEntry) -> WaitKind {
    match sole_condition(entry) {
        None if entry.wait_conditions.is_empty() => WaitKind::Immediate,
        Some(TrainWaitCondition::TimePassed { .. }) => WaitKind::TimePassed,
        Some(TrainWaitCondition::Inactivity { .. }) => WaitKind::Inactivity,
        Some(TrainWaitCondition::CargoFull) => WaitKind::CargoFull,
        Some(TrainWaitCondition::CargoEmpty) => WaitKind::CargoEmpty,
        // Anything the row cannot express starts the cycle from the top, so
        // clicking through it replaces it rather than jamming.
        _ => WaitKind::Immediate,
    }
}

fn wait_ticks(entry: &TrainScheduleEntry) -> u64 {
    match sole_condition(entry) {
        Some(
            TrainWaitCondition::TimePassed { ticks } | TrainWaitCondition::Inactivity { ticks },
        ) => ticks,
        _ => DEFAULT_WAIT_TICKS,
    }
}

fn pressed(interaction: &Interaction) -> bool {
    *interaction == Interaction::Pressed
}

/// The open train's schedule and its id, for an edit to start from.
fn open_schedule(sim: &Simulation, open: &OpenContainer) -> Option<(TrainId, TrainSchedule)> {
    let train_id = sim.rolling_stock_piece(open.rolling_stock?)?.train;
    Some((train_id, sim.train(train_id)?.schedule.clone()))
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

pub(crate) fn handle_schedule_stop_buttons(
    buttons: Query<(&Interaction, &ScheduleStopCycleButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((index, backwards)) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| (button.index, button.backwards))
    else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = open_schedule(&sim, &open_container) else {
        return;
    };
    let names = station_names(&sim);
    let Some(entry) = schedule.entries.get_mut(index) else {
        return;
    };
    // An entry naming a station that no longer answers to anything starts from
    // the first real one; otherwise the cycle moves along the list.
    let next = match names.iter().position(|name| *name == entry.stop_name) {
        Some(current) => cycle(&(0..names.len()).collect::<Vec<_>>(), current, backwards),
        None => 0,
    };
    let Some(name) = names.get(next) else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    entry.stop_name.clone_from(name);
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_condition_buttons(
    buttons: Query<(&Interaction, &ScheduleConditionCycleButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((index, backwards)) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| (button.index, button.backwards))
    else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = open_schedule(&sim, &open_container) else {
        return;
    };
    let Some(entry) = schedule.entries.get_mut(index) else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    let ticks = wait_ticks(entry);
    let kind = cycle(&WaitKind::ALL, wait_kind(entry), backwards);
    entry.wait_conditions = match kind.condition(ticks) {
        Some(condition) => vec![TrainWaitConditionGroup(vec![condition])],
        None => Vec::new(),
    };
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_wait_step_buttons(
    buttons: Query<(&Interaction, &ScheduleWaitStepButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some((index, delta)) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| (button.index, button.delta))
    else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = open_schedule(&sim, &open_container) else {
        return;
    };
    let Some(entry) = schedule.entries.get_mut(index) else {
        return;
    };
    // A wait of no time at all is "leave at once" spelled the long way, so the
    // step stops at one second rather than passing through zero.
    let ticks = wait_ticks(entry)
        .saturating_add_signed(delta)
        .max(WAIT_STEP_TICKS);
    let Some(condition) = sole_condition(entry).and_then(|condition| match condition {
        TrainWaitCondition::TimePassed { .. } => Some(TrainWaitCondition::TimePassed { ticks }),
        TrainWaitCondition::Inactivity { .. } => Some(TrainWaitCondition::Inactivity { ticks }),
        _ => None,
    }) else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    entry.wait_conditions = vec![TrainWaitConditionGroup(vec![condition])];
    send(&mut commands, train_id, schedule);
}

pub(crate) fn handle_schedule_remove_buttons(
    buttons: Query<(&Interaction, &ScheduleRemoveButton), Changed<Interaction>>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some(index) = buttons
        .iter()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| button.0)
    else {
        return;
    };
    let sim = sim.read();
    let Some((train_id, mut schedule)) = open_schedule(&sim, &open_container) else {
        return;
    };
    if index >= schedule.entries.len() {
        return;
    }
    sounds.write(SoundEvent::UiClick);
    schedule.entries.remove(index);
    schedule.current = cursor_after_removing(schedule.current, index, schedule.entries.len());
    send(&mut commands, train_id, schedule);
}

/// Where the schedule cursor lands once the entry at `index` is gone.
///
/// Three cases, and the third is the one worth stating. An entry removed
/// *before* the one being served shifts it down by one, so the train keeps
/// serving the station it is actually standing at. An entry removed at or after
/// it leaves the cursor where it is — which is now the entry that has slid into
/// that slot, the next station of the loop. And a cursor left off the end goes
/// to the top rather than back one, because a schedule is a loop: removing the
/// last entry while it is the one being served sends the train round to the
/// first, the way finishing that entry would have. Stepping back instead would
/// run the loop backwards.
fn cursor_after_removing(current: usize, index: usize, remaining: usize) -> usize {
    let moved = if current > index {
        current - 1
    } else {
        current
    };
    if moved >= remaining { 0 } else { moved }
}

pub(crate) fn handle_schedule_add_button(
    buttons: Query<&Interaction, (Changed<Interaction>, With<ScheduleAddButton>)>,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !buttons.iter().any(pressed) {
        return;
    }
    let sim = sim.read();
    let Some((train_id, mut schedule)) = open_schedule(&sim, &open_container) else {
        return;
    };
    let names = station_names(&sim);
    let Some(name) = names.first() else {
        return;
    };
    sounds.write(SoundEvent::UiClick);
    // A new entry leaves at once, which is the one setting that can never
    // strand the train while the player decides what it should wait for.
    schedule.entries.push(TrainScheduleEntry {
        stop_name: name.clone(),
        wait_conditions: Vec::new(),
    });
    send(&mut commands, train_id, schedule);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cursor has to survive a deletion, because it is what the train does
    /// next. Every case is stated here rather than left to the simulation's
    /// clamp: a clamp keeps the schedule *valid*, and what matters is which
    /// station the train goes to.
    #[test]
    fn removing_an_entry_leaves_the_cursor_on_the_next_station_of_the_loop() {
        // Before the cursor: the train keeps serving the station it is at.
        assert_eq!(cursor_after_removing(2, 0, 2), 1);
        assert_eq!(cursor_after_removing(1, 0, 2), 0);
        // After the cursor: nothing about this journey changed.
        assert_eq!(cursor_after_removing(0, 2, 2), 0);
        // The entry being served, with more of the loop behind it: the cursor
        // stays put, which is the entry that slid into the slot.
        assert_eq!(cursor_after_removing(1, 1, 2), 1);
        // The *last* entry while it is being served: round to the top, the way
        // finishing it would have gone, rather than back to the one before.
        assert_eq!(cursor_after_removing(2, 2, 2), 0);
        // The only entry there was.
        assert_eq!(cursor_after_removing(0, 0, 0), 0);
    }
}
