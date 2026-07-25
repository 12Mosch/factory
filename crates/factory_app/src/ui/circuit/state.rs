use bevy::prelude::*;
use factory_sim::{
    ArithmeticCombinatorState, ArithmeticOperation, CircuitCondition, Comparator,
    DeciderCombinatorState, DeciderOutputValue, EntityId, SignalId, SignalOperand, Simulation,
};

/// Which configuration field a signal picked from the picker lands in. The
/// entity is always the open container, so the target only has to name the
/// field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignalSlot {
    ConditionLeft,
    ConditionRight,
    AccumulatorCharge,
    ConstantSlot(usize),
    ArithmeticLeft,
    ArithmeticRight,
    ArithmeticOutput,
    DeciderLeft,
    DeciderRight,
    DeciderOutput,
}

impl SignalSlot {
    /// Whether the slot holds a [`SignalOperand`], which can also carry a
    /// plain number, rather than a bare signal.
    pub(crate) const fn is_operand(self) -> bool {
        matches!(
            self,
            Self::ConditionRight
                | Self::ArithmeticLeft
                | Self::ArithmeticRight
                | Self::DeciderRight
        )
    }
}

/// Open signal picker, if any. One picker at a time keeps the interaction
/// unambiguous: choosing a signal always fills the slot that opened it.
#[derive(Resource, Default)]
pub(crate) struct CircuitEditorState {
    pub(crate) picker: Option<SignalSlot>,
}

/// The condition currently configured on an entity, or a neutral default the
/// editor can start from.
pub(crate) fn current_condition(sim: &Simulation, entity_id: EntityId) -> Option<CircuitCondition> {
    sim.circuit_entity_state(entity_id)
        .and_then(|state| state.enable_condition)
}

/// A condition to edit: the configured one, or a blank starting point whose
/// left signal is still unset.
pub(crate) fn condition_for_edit(
    sim: &Simulation,
    entity_id: EntityId,
    fallback_left: Option<SignalId>,
) -> Option<CircuitCondition> {
    if let Some(condition) = current_condition(sim, entity_id) {
        return Some(condition);
    }
    Some(CircuitCondition {
        left: fallback_left?,
        comparator: Comparator::Greater,
        right: SignalOperand::Constant(0),
    })
}

pub(crate) fn current_arithmetic(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<ArithmeticCombinatorState> {
    factory_sim::entity_access::arithmetic_combinator_state(sim, entity_id).cloned()
}

pub(crate) fn current_decider(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<DeciderCombinatorState> {
    factory_sim::entity_access::decider_combinator_state(sim, entity_id).cloned()
}

/// Steps a value through a fixed list, wrapping at the end. Used by the
/// comparator and operation buttons, which cycle in place instead of opening a
/// picker because their option lists are short.
pub(crate) fn cycle<T: PartialEq + Copy>(values: &[T], current: T, backwards: bool) -> T {
    let Some(index) = values.iter().position(|value| *value == current) else {
        return values[0];
    };
    let len = values.len();
    let next = if backwards {
        (index + len - 1) % len
    } else {
        (index + 1) % len
    };
    values[next]
}

pub(crate) fn next_comparator(current: Comparator, backwards: bool) -> Comparator {
    cycle(&Comparator::ALL, current, backwards)
}

pub(crate) fn next_operation(current: ArithmeticOperation, backwards: bool) -> ArithmeticOperation {
    cycle(&ArithmeticOperation::ALL, current, backwards)
}

pub(crate) fn toggled_output_value(current: DeciderOutputValue) -> DeciderOutputValue {
    match current {
        DeciderOutputValue::One => DeciderOutputValue::InputCount,
        DeciderOutputValue::InputCount => DeciderOutputValue::One,
    }
}

/// Replaces an operand's constant, keeping a signal operand untouched.
pub(crate) fn operand_with_constant_delta(operand: SignalOperand, delta: i32) -> SignalOperand {
    match operand {
        SignalOperand::Constant(value) => SignalOperand::Constant(value.saturating_add(delta)),
        SignalOperand::Signal(_) => operand,
    }
}

/// Flips an operand between a signal and a plain number, keeping whichever
/// value it already had where possible.
pub(crate) fn toggled_operand_mode(
    operand: SignalOperand,
    fallback_signal: Option<SignalId>,
) -> SignalOperand {
    match operand {
        SignalOperand::Constant(_) => match fallback_signal {
            Some(signal) => SignalOperand::Signal(signal),
            // Without any signal to switch to, staying a constant is better
            // than silently producing an unset operand.
            None => operand,
        },
        SignalOperand::Signal(_) => SignalOperand::Constant(0),
    }
}
