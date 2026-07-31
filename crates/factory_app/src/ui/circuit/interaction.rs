//! Turns circuit-editor clicks into simulation commands.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::{CircuitCondition, EntityId, SignalId, SignalOperand, SimCommand, Simulation};

use crate::audio::SoundEvent;
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::resources::OpenContainer;

use super::state::*;
use super::widgets::*;

#[derive(SystemParam)]
pub(crate) struct CircuitInteractionState<'w> {
    sim: Res<'w, SimResource>,
    open_container: Res<'w, OpenContainer>,
    editor: ResMut<'w, CircuitEditorState>,
    commands: MessageWriter<'w, SimCommandRequest>,
    sounds: MessageWriter<'w, SoundEvent>,
}

type SignalButtons<'w, 's> =
    Query<'w, 's, (&'static Interaction, &'static CircuitSignalButton), Changed<Interaction>>;
type ModeButtons<'w, 's> =
    Query<'w, 's, (&'static Interaction, &'static CircuitOperandModeButton), Changed<Interaction>>;
type ConstantStepButtons<'w, 's> =
    Query<'w, 's, (&'static Interaction, &'static CircuitConstantStepButton), Changed<Interaction>>;
type SlotStepButtons<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static CircuitSlotValueStepButton),
    Changed<Interaction>,
>;

fn pressed(interaction: &Interaction) -> bool {
    *interaction == Interaction::Pressed
}

/// Slot buttons either open the signal picker or, for a slot currently holding
/// a number, do nothing until the mode button switches it back to a signal.
pub(crate) fn handle_circuit_signal_buttons(
    mut buttons: SignalButtons,
    mut state: CircuitInteractionState,
) {
    let Some(slot) = buttons
        .iter_mut()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| button.0)
    else {
        return;
    };
    state.sounds.write(SoundEvent::UiClick);

    if slot.is_operand()
        && let Some(entity_id) = state.open_container.entity_id
        && matches!(
            operand_for_slot(&state.sim.read(), entity_id, slot),
            Some(SignalOperand::Constant(_))
        )
    {
        return;
    }
    // Clicking the slot that is already picking closes the picker again.
    state.editor.picker = (state.editor.picker != Some(slot)).then_some(slot);
}

pub(crate) fn handle_circuit_operand_mode_buttons(
    mut buttons: ModeButtons,
    mut state: CircuitInteractionState,
) {
    let Some(slot) = buttons
        .iter_mut()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| button.0)
    else {
        return;
    };
    let Some(entity_id) = state.open_container.entity_id else {
        return;
    };
    state.sounds.write(SoundEvent::UiClick);

    let command = {
        let sim = state.sim.read();
        let Some(current) = operand_for_slot(&sim, entity_id, slot) else {
            return;
        };
        // Switching to a signal needs some signal to switch to; use the first
        // catalog signal as the default so the flip is never a dead end.
        let fallback = default_signal(&sim);
        command_for_operand(
            &sim,
            entity_id,
            slot,
            toggled_operand_mode(current, fallback),
        )
    };
    if let Some(command) = command {
        state.commands.write(SimCommandRequest(command));
    }
}

pub(crate) fn handle_circuit_constant_step_buttons(
    mut buttons: ConstantStepButtons,
    mut state: CircuitInteractionState,
) {
    let Some(button) = buttons
        .iter_mut()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| (button.slot, button.delta))
    else {
        return;
    };
    let Some(entity_id) = state.open_container.entity_id else {
        return;
    };
    state.sounds.write(SoundEvent::UiClick);

    let command = {
        let sim = state.sim.read();
        let Some(current) = operand_for_slot(&sim, entity_id, button.0) else {
            return;
        };
        command_for_operand(
            &sim,
            entity_id,
            button.0,
            operand_with_constant_delta(current, button.1),
        )
    };
    if let Some(command) = command {
        state.commands.write(SimCommandRequest(command));
    }
}

pub(crate) fn handle_circuit_slot_step_buttons(
    mut buttons: SlotStepButtons,
    mut state: CircuitInteractionState,
) {
    let Some((slot_index, delta)) = buttons
        .iter_mut()
        .find(|(interaction, _)| pressed(interaction))
        .map(|(_, button)| (button.slot_index, button.delta))
    else {
        return;
    };
    let Some(entity_id) = state.open_container.entity_id else {
        return;
    };
    state.sounds.write(SoundEvent::UiClick);

    let slot = {
        let sim = state.sim.read();
        let Some(mut slot) = factory_sim::entity_access::constant_combinator_state(&sim, entity_id)
            .and_then(|state| state.slots.get(slot_index))
            .copied()
        else {
            return;
        };
        slot.value = slot.value.saturating_add(delta);
        slot
    };
    state
        .commands
        .write(SimCommandRequest(SimCommand::SetConstantCombinatorSlot {
            entity_id,
            slot_index,
            slot,
        }));
}

#[derive(SystemParam)]
pub(crate) struct CircuitToggleState<'w> {
    sim: Res<'w, SimResource>,
    open_container: Res<'w, OpenContainer>,
    commands: MessageWriter<'w, SimCommandRequest>,
    sounds: MessageWriter<'w, SoundEvent>,
}

type ComparatorButtons<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<CircuitComparatorButton>)>;
type OperationButtons<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<CircuitOperationButton>)>;
type ClearConditionButtons<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<CircuitClearConditionButton>)>;
type ReadContentsButtons<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<CircuitReadContentsButton>)>;
type ConstantEnabledButtons<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (Changed<Interaction>, With<ConstantCombinatorEnabledButton>),
>;
type OutputValueButtons<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<DeciderOutputValueButton>)>;

/// The buttons whose whole effect is "advance to the next value". Grouped into
/// one system because they share the read-modify-write shape and only ever
/// touch the open entity.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_circuit_toggle_buttons(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut comparators: ComparatorButtons,
    mut operations: OperationButtons,
    mut clears: ClearConditionButtons,
    mut read_contents: ReadContentsButtons,
    mut constant_enabled: ConstantEnabledButtons,
    mut output_values: OutputValueButtons,
    mut state: CircuitToggleState,
) {
    let Some(entity_id) = state.open_container.entity_id else {
        return;
    };
    // Shift steps a cycling button backwards, so a long option list stays
    // reachable from either direction.
    let backwards = keyboard.is_some_and(|keyboard| {
        keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)
    });

    let mut commands = Vec::new();
    {
        let sim = state.sim.read();
        if comparators.iter_mut().any(pressed) {
            commands.extend(cycle_comparator_command(&sim, entity_id, backwards));
        }
        if operations.iter_mut().any(pressed)
            && let Some(current) = current_arithmetic(&sim, entity_id)
        {
            commands.push(SimCommand::ConfigureArithmeticCombinator {
                entity_id,
                left: current.left,
                operation: next_operation(current.operation, backwards),
                right: current.right,
                output: current.output,
            });
        }
        if clears.iter_mut().any(pressed) {
            commands.push(SimCommand::SetCircuitCondition {
                entity_id,
                condition: None,
            });
        }
        if read_contents.iter_mut().any(pressed) {
            let current = sim
                .circuit_entity_state(entity_id)
                .is_some_and(|state| state.read_contents);
            commands.push(SimCommand::SetCircuitReadContents {
                entity_id,
                read_contents: !current,
            });
        }
        if constant_enabled.iter_mut().any(pressed)
            && let Some(current) =
                factory_sim::entity_access::constant_combinator_state(&sim, entity_id)
        {
            commands.push(SimCommand::SetConstantCombinatorEnabled {
                entity_id,
                enabled: !current.enabled,
            });
        }
        if output_values.iter_mut().any(pressed)
            && let Some(current) = current_decider(&sim, entity_id)
        {
            commands.push(SimCommand::ConfigureDeciderCombinator {
                entity_id,
                left: current.left,
                comparator: current.comparator,
                right: current.right,
                output: current.output,
                output_value: toggled_output_value(current.output_value),
            });
        }
    }

    if commands.is_empty() {
        return;
    }
    state.sounds.write(SoundEvent::UiClick);
    for command in commands {
        state.commands.write(SimCommandRequest(command));
    }
}

/// The comparator button serves both the decider combinator and an entity's
/// enable condition; whichever the open entity has is the one that advances.
fn cycle_comparator_command(
    sim: &Simulation,
    entity_id: EntityId,
    backwards: bool,
) -> Option<SimCommand> {
    if let Some(current) = current_decider(sim, entity_id) {
        return Some(SimCommand::ConfigureDeciderCombinator {
            entity_id,
            left: current.left,
            comparator: next_comparator(current.comparator, backwards),
            right: current.right,
            output: current.output,
            output_value: current.output_value,
        });
    }
    let condition = condition_for_edit(sim, entity_id, default_signal(sim))?;
    Some(SimCommand::SetCircuitCondition {
        entity_id,
        condition: Some(CircuitCondition {
            comparator: next_comparator(condition.comparator, backwards),
            ..condition
        }),
    })
}

/// Applies a picked signal to whichever slot opened the picker.
pub(crate) fn command_for_picked_signal(
    sim: &Simulation,
    entity_id: EntityId,
    slot: SignalSlot,
    signal: Option<SignalId>,
) -> Option<SimCommand> {
    match slot {
        SignalSlot::ConditionLeft => {
            let Some(signal) = signal else {
                // Clearing the left signal has no meaningful "unset" form, so
                // it removes the condition entirely.
                return Some(SimCommand::SetCircuitCondition {
                    entity_id,
                    condition: None,
                });
            };
            let condition = condition_for_edit(sim, entity_id, Some(signal))?;
            Some(SimCommand::SetCircuitCondition {
                entity_id,
                condition: Some(CircuitCondition {
                    left: signal,
                    ..condition
                }),
            })
        }
        SignalSlot::EntityOutput => Some(SimCommand::SetEntityOutputSignal { entity_id, signal }),
        // Clearing it hands the stop back to its hand-set limit, which is why
        // the picker's clear button is not a dead end here.
        SignalSlot::TrainStopLimit => Some(SimCommand::SetTrainStopLimitSignal {
            stop: entity_id,
            signal,
        }),
        SignalSlot::ConstantSlot(slot_index) => {
            let mut value = factory_sim::entity_access::constant_combinator_state(sim, entity_id)
                .and_then(|state| state.slots.get(slot_index))
                .copied()?;
            value.signal = signal;
            Some(SimCommand::SetConstantCombinatorSlot {
                entity_id,
                slot_index,
                slot: value,
            })
        }
        SignalSlot::ArithmeticOutput => {
            let current = current_arithmetic(sim, entity_id)?;
            Some(SimCommand::ConfigureArithmeticCombinator {
                entity_id,
                left: current.left,
                operation: current.operation,
                right: current.right,
                output: signal,
            })
        }
        SignalSlot::DeciderLeft => {
            let current = current_decider(sim, entity_id)?;
            Some(SimCommand::ConfigureDeciderCombinator {
                entity_id,
                left: signal,
                comparator: current.comparator,
                right: current.right,
                output: current.output,
                output_value: current.output_value,
            })
        }
        SignalSlot::DeciderOutput => {
            let current = current_decider(sim, entity_id)?;
            Some(SimCommand::ConfigureDeciderCombinator {
                entity_id,
                left: current.left,
                comparator: current.comparator,
                right: current.right,
                output: signal,
                output_value: current.output_value,
            })
        }
        SignalSlot::LogisticRequest(slot_index) => {
            // Only an item can sit in a chest row, and clearing the item clears
            // the amount with it so no orphaned number survives the change.
            let item = match signal {
                Some(SignalId::Item(item_id)) => Some(item_id),
                Some(_) => return None,
                None => None,
            };
            let count = item
                .and_then(|_| {
                    sim.logistic_chest_state(entity_id)?
                        .requests
                        .get(slot_index)
                        .map(|request| request.count)
                })
                .unwrap_or_default();
            Some(SimCommand::SetLogisticRequest {
                entity_id,
                slot_index,
                request: factory_sim::LogisticRequest { item, count },
            })
        }
        SignalSlot::ConditionRight
        | SignalSlot::ArithmeticLeft
        | SignalSlot::ArithmeticRight
        | SignalSlot::DeciderRight => {
            let operand = match signal {
                Some(signal) => SignalOperand::Signal(signal),
                None => SignalOperand::Constant(0),
            };
            command_for_operand(sim, entity_id, slot, operand)
        }
    }
}

/// Current value of an operand slot.
fn operand_for_slot(
    sim: &Simulation,
    entity_id: EntityId,
    slot: SignalSlot,
) -> Option<SignalOperand> {
    match slot {
        SignalSlot::ConditionRight => Some(
            current_condition(sim, entity_id)
                .map(|condition| condition.right)
                .unwrap_or(SignalOperand::Constant(0)),
        ),
        SignalSlot::ArithmeticLeft => current_arithmetic(sim, entity_id).map(|state| state.left),
        SignalSlot::ArithmeticRight => current_arithmetic(sim, entity_id).map(|state| state.right),
        SignalSlot::DeciderRight => current_decider(sim, entity_id).map(|state| state.right),
        _ => None,
    }
}

/// Rewrites one operand slot, preserving the rest of the entity's config.
fn command_for_operand(
    sim: &Simulation,
    entity_id: EntityId,
    slot: SignalSlot,
    operand: SignalOperand,
) -> Option<SimCommand> {
    match slot {
        SignalSlot::ConditionRight => {
            let condition = condition_for_edit(sim, entity_id, default_signal(sim))?;
            Some(SimCommand::SetCircuitCondition {
                entity_id,
                condition: Some(CircuitCondition {
                    right: operand,
                    ..condition
                }),
            })
        }
        SignalSlot::ArithmeticLeft | SignalSlot::ArithmeticRight => {
            let current = current_arithmetic(sim, entity_id)?;
            let (left, right) = if slot == SignalSlot::ArithmeticLeft {
                (operand, current.right)
            } else {
                (current.left, operand)
            };
            Some(SimCommand::ConfigureArithmeticCombinator {
                entity_id,
                left,
                operation: current.operation,
                right,
                output: current.output,
            })
        }
        SignalSlot::DeciderRight => {
            let current = current_decider(sim, entity_id)?;
            Some(SimCommand::ConfigureDeciderCombinator {
                entity_id,
                left: current.left,
                comparator: current.comparator,
                right: operand,
                output: current.output,
                output_value: current.output_value,
            })
        }
        _ => None,
    }
}

/// A signal to seed a freshly created condition with. The first catalog item
/// is arbitrary but always valid, and the player replaces it from the picker.
fn default_signal(sim: &Simulation) -> Option<SignalId> {
    sim.catalog()
        .items
        .first()
        .map(|item| SignalId::Item(item.id))
}
