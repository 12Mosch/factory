//! Builds the circuit editors and keeps their labels in sync with the
//! simulation.

use bevy::prelude::*;
use factory_data::CombinatorKind;
use factory_sim::{
    ArithmeticOperation, Comparator, ConnectorPort, DeciderOutputValue, EntityId, SignalOperand,
    Simulation,
};

use crate::resources::SimResource;
use crate::ui::resources::OpenContainer;

use super::signals::{optional_signal_label, signal_display_name};
use super::state::SignalSlot;
use super::widgets::*;

/// Longest signal list shown in the "on this network" summary. Networks can
/// carry a hundred channels; the panel only needs enough to confirm wiring.
const MAX_SUMMARY_SIGNALS: usize = 6;

/// Circuit section for an ordinary entity: contents publishing plus the
/// enable/disable condition.
pub(crate) fn spawn_circuit_control_panel(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    connector: factory_data::CircuitConnectorPrototype,
    is_accumulator: bool,
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
            spawn_heading(panel, "Circuit");
            if connector.reads_contents {
                spawn_row(panel, |row| {
                    spawn_caption(row, "Read contents");
                    spawn_button(
                        row,
                        44.0,
                        "Off",
                        CircuitReadContentsButton,
                        CircuitLabel(CircuitLabelKind::ReadContents),
                    );
                });
            }
            if is_accumulator {
                spawn_row(panel, |row| {
                    spawn_caption(row, "Charge signal");
                    spawn_button(
                        row,
                        50.0,
                        "--",
                        CircuitSignalButton(SignalSlot::AccumulatorCharge),
                        CircuitLabel(CircuitLabelKind::Signal(SignalSlot::AccumulatorCharge)),
                    );
                });
            }
            if connector.controllable {
                spawn_caption(panel, "Enable when");
                spawn_condition_row(panel);
            }
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                CircuitLabel(CircuitLabelKind::Status),
            ));
        });
}

fn spawn_condition_row(panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands) {
    spawn_row(panel, |row| {
        spawn_button(
            row,
            50.0,
            "--",
            CircuitSignalButton(SignalSlot::ConditionLeft),
            CircuitLabel(CircuitLabelKind::Signal(SignalSlot::ConditionLeft)),
        );
        spawn_button(
            row,
            32.0,
            ">",
            CircuitComparatorButton,
            CircuitLabel(CircuitLabelKind::Comparator),
        );
        spawn_operand(row, SignalSlot::ConditionRight);
    });
    spawn_row(panel, |row| {
        spawn_stepper(row, |delta| CircuitConstantStepButton {
            slot: SignalSlot::ConditionRight,
            delta,
        });
        spawn_button(row, 46.0, "Always", CircuitClearConditionButton, ());
    });
}

/// A signal-or-number operand: a mode toggle plus the value it holds.
fn spawn_operand(row: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, slot: SignalSlot) {
    spawn_button(
        row,
        34.0,
        "NUM",
        CircuitOperandModeButton(slot),
        CircuitLabel(CircuitLabelKind::OperandMode(slot)),
    );
    spawn_button(
        row,
        56.0,
        "0",
        CircuitSignalButton(slot),
        CircuitLabel(CircuitLabelKind::Signal(slot)),
    );
}

pub(crate) fn spawn_constant_combinator_panel(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    slot_count: usize,
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
            spawn_heading(panel, "Constant Combinator");
            spawn_row(panel, |row| {
                spawn_caption(row, "Output");
                spawn_button(
                    row,
                    44.0,
                    "On",
                    ConstantCombinatorEnabledButton,
                    CircuitLabel(CircuitLabelKind::ConstantEnabled),
                );
            });
            for slot_index in 0..slot_count {
                spawn_row(panel, |row| {
                    spawn_button(
                        row,
                        50.0,
                        "--",
                        CircuitSignalButton(SignalSlot::ConstantSlot(slot_index)),
                        CircuitLabel(CircuitLabelKind::Signal(SignalSlot::ConstantSlot(
                            slot_index,
                        ))),
                    );
                    row.spawn((
                        Text::new("0".to_string()),
                        TextFont::from_font_size(10.0),
                        TextColor(Color::WHITE),
                        Node {
                            width: Val::Px(44.0),
                            ..default()
                        },
                        CircuitLabel(CircuitLabelKind::SlotValue(slot_index)),
                    ));
                    spawn_stepper(row, |delta| CircuitSlotValueStepButton {
                        slot_index,
                        delta,
                    });
                });
            }
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                CircuitLabel(CircuitLabelKind::Status),
            ));
        });
}

pub(crate) fn spawn_arithmetic_combinator_panel(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
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
            spawn_heading(panel, "Arithmetic Combinator");
            spawn_row(panel, |row| {
                spawn_operand(row, SignalSlot::ArithmeticLeft);
                spawn_button(
                    row,
                    34.0,
                    "+",
                    CircuitOperationButton,
                    CircuitLabel(CircuitLabelKind::Operation),
                );
                spawn_operand(row, SignalSlot::ArithmeticRight);
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Left");
                spawn_stepper(row, |delta| CircuitConstantStepButton {
                    slot: SignalSlot::ArithmeticLeft,
                    delta,
                });
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Right");
                spawn_stepper(row, |delta| CircuitConstantStepButton {
                    slot: SignalSlot::ArithmeticRight,
                    delta,
                });
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Output");
                spawn_button(
                    row,
                    50.0,
                    "--",
                    CircuitSignalButton(SignalSlot::ArithmeticOutput),
                    CircuitLabel(CircuitLabelKind::Signal(SignalSlot::ArithmeticOutput)),
                );
            });
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                CircuitLabel(CircuitLabelKind::Status),
            ));
        });
}

pub(crate) fn spawn_decider_combinator_panel(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
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
            spawn_heading(panel, "Decider Combinator");
            spawn_row(panel, |row| {
                spawn_button(
                    row,
                    50.0,
                    "--",
                    CircuitSignalButton(SignalSlot::DeciderLeft),
                    CircuitLabel(CircuitLabelKind::Signal(SignalSlot::DeciderLeft)),
                );
                spawn_button(
                    row,
                    32.0,
                    ">",
                    CircuitComparatorButton,
                    CircuitLabel(CircuitLabelKind::Comparator),
                );
                spawn_operand(row, SignalSlot::DeciderRight);
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Right");
                spawn_stepper(row, |delta| CircuitConstantStepButton {
                    slot: SignalSlot::DeciderRight,
                    delta,
                });
            });
            spawn_row(panel, |row| {
                spawn_caption(row, "Output");
                spawn_button(
                    row,
                    50.0,
                    "--",
                    CircuitSignalButton(SignalSlot::DeciderOutput),
                    CircuitLabel(CircuitLabelKind::Signal(SignalSlot::DeciderOutput)),
                );
                spawn_button(
                    row,
                    52.0,
                    "1",
                    DeciderOutputValueButton,
                    CircuitLabel(CircuitLabelKind::DeciderOutputValue),
                );
            });
            panel.spawn((
                Text::new(String::new()),
                TextFont::from_font_size(10.0),
                TextColor(LABEL_COLOR),
                CircuitLabel(CircuitLabelKind::Status),
            ));
        });
}

/// Refreshes every live label in the open circuit editor.
///
/// One query over [`CircuitLabel`] rather than one per field: they all write
/// `Text`, and separate marker components would leave Bevy unable to prove the
/// queries disjoint.
pub(crate) fn update_circuit_panel(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut labels: Query<(&CircuitLabel, &mut Text)>,
) {
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    let sim = sim.read();
    let catalog = sim.catalog();
    // Built once per frame because every status label shows the same summary.
    let mut status: Option<String> = None;

    for (label, mut text) in &mut labels {
        text.0 = match label.0 {
            CircuitLabelKind::Signal(slot) => match slot_value(&sim, entity_id, slot) {
                SlotValue::Signal(signal) => optional_signal_label(catalog, signal),
                SlotValue::Constant(value) => value.to_string(),
                SlotValue::Missing => "--".to_string(),
            },
            CircuitLabelKind::OperandMode(slot) => match slot_value(&sim, entity_id, slot) {
                SlotValue::Constant(_) => "NUM".to_string(),
                SlotValue::Signal(_) => "SIG".to_string(),
                SlotValue::Missing => "--".to_string(),
            },
            CircuitLabelKind::SlotValue(index) => {
                factory_sim::entity_access::constant_combinator_state(&sim, entity_id)
                    .and_then(|state| state.slots.get(index))
                    .map(|slot| slot.value.to_string())
                    .unwrap_or_else(|| "0".to_string())
            }
            CircuitLabelKind::Comparator => {
                current_comparator(&sim, entity_id).symbol().to_string()
            }
            CircuitLabelKind::Operation => {
                factory_sim::entity_access::arithmetic_combinator_state(&sim, entity_id)
                    .map(|state| state.operation)
                    .unwrap_or(ArithmeticOperation::Add)
                    .symbol()
                    .to_string()
            }
            CircuitLabelKind::ReadContents => on_off(
                sim.circuit_entity_state(entity_id)
                    .is_some_and(|state| state.read_contents),
            ),
            CircuitLabelKind::ConstantEnabled => on_off(
                factory_sim::entity_access::constant_combinator_state(&sim, entity_id)
                    .is_some_and(|state| state.enabled),
            ),
            CircuitLabelKind::DeciderOutputValue => {
                match factory_sim::entity_access::decider_combinator_state(&sim, entity_id)
                    .map(|state| state.output_value)
                    .unwrap_or(DeciderOutputValue::One)
                {
                    DeciderOutputValue::One => "1",
                    DeciderOutputValue::InputCount => "count",
                }
                .to_string()
            }
            CircuitLabelKind::Status => status
                .get_or_insert_with(|| circuit_status_text(&sim, entity_id))
                .clone(),
        };
    }
}

fn on_off(value: bool) -> String {
    if value { "On" } else { "Off" }.to_string()
}

/// What a slot currently holds. `Missing` covers slots whose entity has no
/// matching configuration, which is normal while a panel is being rebuilt.
enum SlotValue {
    Signal(Option<factory_sim::SignalId>),
    Constant(i32),
    Missing,
}

fn slot_value(sim: &Simulation, entity_id: EntityId, slot: SignalSlot) -> SlotValue {
    let operand = |operand: SignalOperand| match operand {
        SignalOperand::Signal(signal) => SlotValue::Signal(Some(signal)),
        SignalOperand::Constant(value) => SlotValue::Constant(value),
    };
    match slot {
        SignalSlot::ConditionLeft => SlotValue::Signal(
            sim.circuit_entity_state(entity_id)
                .and_then(|state| state.enable_condition)
                .map(|condition| condition.left),
        ),
        SignalSlot::ConditionRight => sim
            .circuit_entity_state(entity_id)
            .and_then(|state| state.enable_condition)
            .map_or(SlotValue::Constant(0), |condition| operand(condition.right)),
        SignalSlot::AccumulatorCharge => SlotValue::Signal(
            sim.circuit_entity_state(entity_id)
                .and_then(|state| state.charge_output_signal),
        ),
        SignalSlot::ConstantSlot(index) => SlotValue::Signal(
            factory_sim::entity_access::constant_combinator_state(sim, entity_id)
                .and_then(|state| state.slots.get(index))
                .and_then(|slot| slot.signal),
        ),
        SignalSlot::ArithmeticLeft => {
            factory_sim::entity_access::arithmetic_combinator_state(sim, entity_id)
                .map_or(SlotValue::Missing, |state| operand(state.left))
        }
        SignalSlot::ArithmeticRight => {
            factory_sim::entity_access::arithmetic_combinator_state(sim, entity_id)
                .map_or(SlotValue::Missing, |state| operand(state.right))
        }
        SignalSlot::ArithmeticOutput => SlotValue::Signal(
            factory_sim::entity_access::arithmetic_combinator_state(sim, entity_id)
                .and_then(|state| state.output),
        ),
        SignalSlot::DeciderLeft => SlotValue::Signal(
            factory_sim::entity_access::decider_combinator_state(sim, entity_id)
                .and_then(|state| state.left),
        ),
        SignalSlot::DeciderRight => {
            factory_sim::entity_access::decider_combinator_state(sim, entity_id)
                .map_or(SlotValue::Missing, |state| operand(state.right))
        }
        SignalSlot::DeciderOutput => SlotValue::Signal(
            factory_sim::entity_access::decider_combinator_state(sim, entity_id)
                .and_then(|state| state.output),
        ),
        SignalSlot::LogisticRequest(index) => SlotValue::Signal(
            sim.logistic_chest_state(entity_id)
                .and_then(|state| state.requests.get(index))
                .and_then(|request| request.item)
                .map(factory_sim::SignalId::Item),
        ),
    }
}

fn current_comparator(sim: &Simulation, entity_id: EntityId) -> Comparator {
    if let Some(state) = factory_sim::entity_access::decider_combinator_state(sim, entity_id) {
        return state.comparator;
    }
    sim.circuit_entity_state(entity_id)
        .and_then(|state| state.enable_condition)
        .map(|condition| condition.comparator)
        .unwrap_or(Comparator::Greater)
}

/// Human-readable summary of what the entity's connectors currently see.
fn circuit_status_text(sim: &Simulation, entity_id: EntityId) -> String {
    let catalog = sim.catalog();
    let combinator = sim
        .entities()
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
        .and_then(|prototype| prototype.combinator);
    let (port, port_name) = match combinator.map(|combinator| combinator.kind) {
        // A constant combinator has nothing on its input worth reporting, so
        // both it and the other combinators summarize what they emit.
        Some(CombinatorKind::Constant) => (ConnectorPort::Output, "Output"),
        Some(_) => (ConnectorPort::Input, "Input"),
        None => (ConnectorPort::Single, "Network"),
    };

    let signals = sim.circuit_signals_at_node(factory_sim::CircuitNode::new(entity_id, port));
    if signals.is_empty() {
        return format!("{port_name}: no signals");
    }
    let mut parts = signals
        .iter()
        .take(MAX_SUMMARY_SIGNALS)
        .map(|(signal, value)| format!("{} {value}", signal_display_name(catalog, signal)))
        .collect::<Vec<_>>();
    if signals.len() > MAX_SUMMARY_SIGNALS {
        parts.push(format!("+{} more", signals.len() - MAX_SUMMARY_SIGNALS));
    }
    format!("{port_name}: {}", parts.join(", "))
}
