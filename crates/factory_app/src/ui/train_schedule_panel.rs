//! The schedule editor: the ordered list of stations a train serves and the
//! conditions that keep it at each of them.
//!
//! Shown inside the rolling-stock window, because a schedule belongs to the
//! *train* rather than to any one piece of it: opening any wagon of a train
//! shows the same list, and editing it from a wagon is editing the train.
//!
//! Three decisions shape the controls.
//!
//! * **Stations are chosen, not typed.** An entry names a station by name, and
//!   a name that answers to nothing is a train with nowhere to go, so the entry
//!   is filled from a list of the names that exist. A list rather than the
//!   `< name >` cycle it replaces: cycling is fine for three stations and
//!   unusable for thirty, and it never shows the player what the alternatives
//!   are. Renaming a station is still done at the station, where the
//!   consequences of it are visible.
//! * **The editor says everything the simulation stores.** Wait conditions are
//!   ORs of ANDs, and every one of them — a time, an idle spell, a full or
//!   empty load, an item or fluid count, a comparison against the signals
//!   reaching the stop — can be written here. An editor that could only express
//!   the simple half would quietly flatten the rest, so the panel grew the
//!   nesting instead of hiding from it.
//! * **A row shows only controls that do something.** "Wait until the cargo is
//!   full" has no number in it and no channel; a circuit condition compared
//!   against a signal has no number either. Spawning a stepper there would be a
//!   button that answers nothing.
//!
//! Every edit reads the schedule the simulation currently holds, applies one
//! change to a copy, and sends the copy. Nothing here keeps a draft: a draft is
//! a second version of the schedule the panel could show and the train could
//! not be running, and the two would disagree the moment anything else touched
//! the train. That also means only *one* press may be acted on per frame, which
//! is why each handler takes the first press it finds.

use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::prelude::*;
use factory_sim::{
    RollingStockId, SignalOperand, Simulation, TrainId, TrainWaitCondition, TrainWaitConditionKind,
};

use crate::resources::SimResource;
use crate::ui::circuit::signals::signal_short_label;
use crate::ui::circuit::widgets::{
    LABEL_COLOR, spawn_button, spawn_caption, spawn_heading, spawn_row, spawn_wrapping_row,
};
use crate::ui::resources::OpenContainer;

/// Ticks a step button moves a timed condition by: one second, the unit a
/// player thinks about a station wait in.
pub(crate) const WAIT_STEP_TICKS: u64 = 60;

/// Milliunits in one unit of fluid, matching how every other panel shows a
/// fluid amount.
pub(crate) const MILLIUNITS_PER_UNIT: i32 = 1_000;

/// Where one condition sits in a schedule: which stop, which OR alternative,
/// and which AND condition within it.
///
/// An address rather than a handle, because the panel is built from one frame's
/// schedule and clicked in a later one. An address alone cannot say whether it
/// still names what it was written for — remove the first of two conditions and
/// the second answers to the first one's address — so every button carries the
/// [`ScheduleRevision`] it was drawn with, and a press whose revision has since
/// moved on is dropped rather than applied to whatever took its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConditionRef {
    pub(crate) entry: usize,
    pub(crate) group: usize,
    pub(crate) index: usize,
}

/// Which half of a condition a picked signal lands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConditionPart {
    /// What is being counted: the item, the fluid, or a circuit condition's
    /// left-hand signal.
    Subject,
    /// A circuit condition's right-hand operand, which may hold a signal or a
    /// plain number instead.
    CircuitRight,
}

/// The condition slot an open signal picker is filling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConditionSlot {
    pub(crate) condition: ConditionRef,
    pub(crate) part: ConditionPart,
}

#[derive(Component)]
pub(crate) struct ScheduleStatusText;

/// Opens the station list for an entry. An index one past the end appends.
#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleStationButton(pub(crate) usize);

#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleRemoveButton(pub(crate) usize);

/// Hands the train back to its schedule, or takes it off. Carries the mode it
/// was drawn in, so the press asks for the other one.
#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleManualButton(pub(crate) bool);

/// Adds an OR alternative to an entry, with one condition in it.
#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleAddGroupButton(pub(crate) usize);

/// Adds an AND condition to an alternative that already exists.
#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleAddConditionButton {
    pub(crate) entry: usize,
    pub(crate) group: usize,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleConditionKindButton(pub(crate) ConditionRef);

#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleComparatorButton(pub(crate) ConditionRef);

/// Switches a circuit condition's right-hand operand between a signal and a
/// plain number.
#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleOperandModeButton(pub(crate) ConditionRef);

#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleConditionStepButton {
    pub(crate) condition: ConditionRef,
    pub(crate) delta: i32,
}

#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleConditionRemoveButton(pub(crate) ConditionRef);

/// Opens the signal grid for one half of a condition.
#[derive(Component, Clone, Copy)]
pub(crate) struct ScheduleChannelButton(pub(crate) ConditionSlot);

/// One condition as the editor draws it: what it is, what it is about, and what
/// it is compared against — each `None` where the kind has no such part, so the
/// view spawns no control the press could not answer.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ConditionSnapshot {
    kind: TrainWaitConditionKind,
    kind_label: &'static str,
    /// The channel the condition counts, for kinds that name one.
    channel: Option<String>,
    comparator: Option<&'static str>,
    /// `NUM`/`SIG` for a circuit condition's right-hand operand.
    operand_mode: Option<&'static str>,
    /// What the condition compares against, as it reads on the row.
    value: Option<String>,
    /// Whether the stepper has a number to move.
    steppable: bool,
}

/// A row of the editor as it was built: what the entry says, so the window
/// rebuilds when an entry or a condition changes and stays put while the train
/// runs.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ScheduleRowSnapshot {
    pub(crate) stop_name: String,
    /// OR alternatives, each a run of ANDed conditions.
    pub(crate) groups: Vec<Vec<ConditionSnapshot>>,
}

/// What the schedule editor was built from.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ScheduleSnapshot {
    /// Whether the player is driving, in which case the list below is what the
    /// train will do once it is handed back rather than what it is doing.
    pub(crate) manual: bool,
    pub(crate) rows: Vec<ScheduleRowSnapshot>,
    /// Whether any station exists to name. With none, the editor says so
    /// instead of offering an "add" button that could only add nothing.
    pub(crate) has_stations: bool,
}

impl ScheduleSnapshot {
    /// A short stand-in for the whole snapshot, cheap enough to hang off every
    /// button the panel spawns.
    ///
    /// Hashed from the snapshot rather than from the schedule because the
    /// snapshot is what the panel was drawn from, and the window is rebuilt
    /// exactly when the snapshot changes. A schedule that changed without
    /// changing the snapshot left the panel correct, and its buttons must keep
    /// working.
    pub(crate) fn revision(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// The snapshot a button was drawn from, so a press can be told apart from one
/// meant for a panel that no longer exists.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScheduleRevision(pub(crate) u64);

/// What the editor shows for a train, or `None` for a piece of stock that is
/// not part of one.
pub(crate) fn schedule_snapshot(
    sim: &Simulation,
    stock_id: RollingStockId,
) -> Option<ScheduleSnapshot> {
    let train_id = sim.rolling_stock_piece(stock_id)?.train;
    let train = sim.train(train_id)?;
    Some(ScheduleSnapshot {
        manual: train.manual,
        rows: train
            .schedule
            .entries
            .iter()
            .map(|entry| ScheduleRowSnapshot {
                stop_name: entry.stop_name.clone(),
                groups: entry
                    .wait_conditions
                    .iter()
                    .map(|group| {
                        group
                            .0
                            .iter()
                            .map(|condition| condition_snapshot(sim, *condition))
                            .collect()
                    })
                    .collect(),
            })
            .collect(),
        has_stations: !station_names(sim).is_empty(),
    })
}

fn condition_snapshot(sim: &Simulation, condition: TrainWaitCondition) -> ConditionSnapshot {
    let catalog = sim.catalog();
    let short = |signal| signal_short_label(catalog, signal);
    ConditionSnapshot {
        kind: condition.kind(),
        kind_label: condition_kind_label(condition.kind()),
        channel: match condition {
            TrainWaitCondition::ItemCount { item, .. } => {
                Some(short(factory_sim::SignalId::Item(item)))
            }
            TrainWaitCondition::FluidCount { fluid, .. } => {
                Some(short(factory_sim::SignalId::Fluid(fluid)))
            }
            TrainWaitCondition::Circuit(inner) => Some(short(inner.left)),
            _ => None,
        },
        comparator: condition.comparator().map(|comparator| comparator.symbol()),
        operand_mode: match condition {
            TrainWaitCondition::Circuit(inner) => Some(match inner.right {
                SignalOperand::Constant(_) => "NUM",
                SignalOperand::Signal(_) => "SIG",
            }),
            _ => None,
        },
        value: match condition {
            TrainWaitCondition::TimePassed { ticks } | TrainWaitCondition::Inactivity { ticks } => {
                Some(seconds(ticks))
            }
            TrainWaitCondition::ItemCount { count, .. } => Some(count.to_string()),
            TrainWaitCondition::FluidCount { milliunits, .. } => {
                Some((milliunits / MILLIUNITS_PER_UNIT).to_string())
            }
            TrainWaitCondition::Circuit(inner) => Some(match inner.right {
                SignalOperand::Constant(value) => value.to_string(),
                SignalOperand::Signal(signal) => short(signal),
            }),
            _ => None,
        },
        // A circuit condition compared against a signal reads whatever is on
        // the wire, so there is no number on that row for a stepper to move.
        steppable: match condition {
            TrainWaitCondition::Circuit(inner) => {
                matches!(inner.right, SignalOperand::Constant(_))
            }
            TrainWaitCondition::CargoFull | TrainWaitCondition::CargoEmpty => false,
            _ => true,
        },
    }
}

/// What a wait condition's kind is called on its button.
pub(crate) const fn condition_kind_label(kind: TrainWaitConditionKind) -> &'static str {
    match kind {
        TrainWaitConditionKind::CargoFull => "Full",
        TrainWaitConditionKind::CargoEmpty => "Empty",
        TrainWaitConditionKind::TimePassed => "Time",
        TrainWaitConditionKind::Inactivity => "Idle",
        TrainWaitConditionKind::ItemCount => "Item",
        TrainWaitConditionKind::FluidCount => "Fluid",
        TrainWaitConditionKind::Circuit => "Signal",
    }
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
            let revision = ScheduleRevision(snapshot.revision());
            spawn_heading(panel, "Schedule");
            // Driving a train takes it off its orders, and something has to
            // give it back — otherwise a train driven once is a train that
            // never runs its schedule again.
            spawn_row(panel, |controls| {
                spawn_button(
                    controls,
                    88.0,
                    if snapshot.manual {
                        "Manual"
                    } else {
                        "Automatic"
                    },
                    ScheduleManualButton(snapshot.manual),
                    revision,
                );
                spawn_caption(
                    controls,
                    if snapshot.manual {
                        "driven by hand"
                    } else {
                        "running its schedule"
                    },
                );
            });
            for (index, row) in snapshot.rows.iter().enumerate() {
                spawn_entry(panel, index, row, revision);
            }
            if snapshot.has_stations {
                spawn_row(panel, |controls| {
                    spawn_button(
                        controls,
                        66.0,
                        "Add stop",
                        ScheduleStationButton(snapshot.rows.len()),
                        revision,
                    );
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

fn spawn_entry(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    index: usize,
    row: &ScheduleRowSnapshot,
    revision: ScheduleRevision,
) {
    spawn_row(panel, |controls| {
        spawn_button(
            controls,
            140.0,
            &row.stop_name,
            ScheduleStationButton(index),
            revision,
        );
        spawn_button(controls, 18.0, "x", ScheduleRemoveButton(index), revision);
    });
    if row.groups.is_empty() {
        spawn_caption(panel, "  Leaves as soon as it arrives");
    }
    for (group_index, group) in row.groups.iter().enumerate() {
        if group_index > 0 {
            spawn_caption(panel, "  — or —");
        }
        for (condition_index, condition) in group.iter().enumerate() {
            spawn_condition(
                panel,
                ConditionRef {
                    entry: index,
                    group: group_index,
                    index: condition_index,
                },
                condition,
                condition_index > 0,
                revision,
            );
        }
        spawn_row(panel, |controls| {
            spawn_caption(controls, "   ");
            spawn_button(
                controls,
                44.0,
                "+ and",
                ScheduleAddConditionButton {
                    entry: index,
                    group: group_index,
                },
                revision,
            );
        });
    }
    spawn_row(panel, |controls| {
        spawn_caption(controls, "   ");
        // The same button, named for what it does here: an entry with nothing
        // on it is gaining its first condition rather than an alternative to
        // one, and calling that "or" would describe a choice between one thing
        // and nothing.
        spawn_button(
            controls,
            44.0,
            if row.groups.is_empty() {
                "+ wait"
            } else {
                "+ or"
            },
            ScheduleAddGroupButton(index),
            revision,
        );
    });
}

fn spawn_condition(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    address: ConditionRef,
    condition: &ConditionSnapshot,
    leads_with_and: bool,
    revision: ScheduleRevision,
) {
    // Wrapping rather than fixed: a circuit condition compared against a number
    // carries a kind, a channel, a comparator, an operand mode, a value, two
    // steppers and a remove button, which is wider than the panel. The narrow
    // conditions still take one line.
    spawn_wrapping_row(panel, |controls| {
        spawn_caption(controls, if leads_with_and { "and" } else { "   " });
        spawn_button(
            controls,
            42.0,
            condition.kind_label,
            ScheduleConditionKindButton(address),
            revision,
        );
        if let Some(channel) = &condition.channel {
            spawn_button(
                controls,
                44.0,
                channel,
                ScheduleChannelButton(ConditionSlot {
                    condition: address,
                    part: ConditionPart::Subject,
                }),
                revision,
            );
        }
        if let Some(comparator) = condition.comparator {
            spawn_button(
                controls,
                24.0,
                comparator,
                ScheduleComparatorButton(address),
                revision,
            );
        }
        if let Some(mode) = condition.operand_mode {
            spawn_button(
                controls,
                30.0,
                mode,
                ScheduleOperandModeButton(address),
                revision,
            );
        }
        if let Some(value) = &condition.value {
            // A circuit operand holding a signal is picked rather than typed,
            // so its value doubles as the button that opens the grid.
            if condition.kind == TrainWaitConditionKind::Circuit {
                spawn_button(
                    controls,
                    44.0,
                    value,
                    ScheduleChannelButton(ConditionSlot {
                        condition: address,
                        part: ConditionPart::CircuitRight,
                    }),
                    revision,
                );
            } else {
                controls.spawn((
                    Node {
                        width: Val::Px(44.0),
                        ..default()
                    },
                    Text::new(value.clone()),
                    TextFont::from_font_size(10.0),
                    TextColor(LABEL_COLOR),
                ));
            }
        }
        if condition.steppable {
            for delta in [-1, 1] {
                spawn_button(
                    controls,
                    18.0,
                    if delta > 0 { "+" } else { "-" },
                    ScheduleConditionStepButton {
                        condition: address,
                        delta,
                    },
                    revision,
                );
            }
        }
        spawn_button(
            controls,
            18.0,
            "x",
            ScheduleConditionRemoveButton(address),
            revision,
        );
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
/// First occurrence wins rather than sorting, so the list does not reorder
/// itself when a station is renamed.
pub(crate) fn station_names(sim: &Simulation) -> Vec<String> {
    let mut names = Vec::new();
    for (_, state) in sim.train_stops() {
        if !names.contains(&state.name) {
            names.push(state.name.clone());
        }
    }
    names
}

fn seconds(ticks: u64) -> String {
    if ticks == u64::MAX {
        return "for ever".to_string();
    }
    format!("{}s", ticks / WAIT_STEP_TICKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_of(sim: &Simulation, conditions: &[TrainWaitCondition]) -> ScheduleSnapshot {
        ScheduleSnapshot {
            manual: false,
            rows: vec![ScheduleRowSnapshot {
                stop_name: "Depot".into(),
                groups: vec![
                    conditions
                        .iter()
                        .map(|condition| condition_snapshot(sim, *condition))
                        .collect(),
                ],
            }],
            has_stations: true,
        }
    }

    /// The press this revision exists to refuse.
    ///
    /// A schedule edit reaches the simulation a frame before the panel is
    /// redrawn, so for that frame the buttons on screen still address the
    /// schedule as it was. Remove the first of two conditions and the second
    /// slides into its address: a second press of the same stale button finds
    /// something there and would remove it too. It carries the revision it was
    /// drawn under, which no longer matches, so the press is dropped instead.
    #[test]
    fn removing_a_condition_leaves_the_buttons_drawn_beside_it_stale() {
        let sim = Simulation::new_test_world(1);
        let waited = TrainWaitCondition::TimePassed { ticks: 60 };
        let before = snapshot_of(&sim, &[TrainWaitCondition::CargoFull, waited]);
        let after = snapshot_of(&sim, &[waited]);

        assert_ne!(
            before.revision(),
            after.revision(),
            "a condition shifting into another's address must not go unnoticed"
        );
    }

    /// The other half of the bargain: a revision that rejected ordinary presses
    /// would be a schedule editor with dead buttons.
    #[test]
    fn a_schedule_that_did_not_change_keeps_its_buttons_live() {
        let sim = Simulation::new_test_world(1);
        let conditions = [TrainWaitCondition::CargoEmpty];

        assert_eq!(
            snapshot_of(&sim, &conditions).revision(),
            snapshot_of(&sim, &conditions).revision(),
            "the same panel drawn twice must answer its own presses"
        );
    }
}
