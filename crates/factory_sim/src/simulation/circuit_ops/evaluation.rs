use super::*;

/// How a wildcard signal behaves when it appears in a combinator or condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) enum SignalRole {
    /// An ordinary channel with a single value.
    Value,
    /// Runs the rule once per input signal.
    Each,
    /// Passes when at least one input signal satisfies the comparison.
    Anything,
    /// Passes when every input signal satisfies the comparison.
    Everything,
}

impl Simulation {
    pub(in crate::simulation) fn signal_role(&self, signal: SignalId) -> SignalRole {
        let SignalId::Virtual(virtual_id) = signal else {
            return SignalRole::Value;
        };
        match self
            .world
            .prototypes
            .virtual_signal(virtual_id)
            .map(|prototype| prototype.kind)
        {
            Some(factory_data::VirtualSignalKind::Each) => SignalRole::Each,
            Some(factory_data::VirtualSignalKind::Anything) => SignalRole::Anything,
            Some(factory_data::VirtualSignalKind::Everything) => SignalRole::Everything,
            _ => SignalRole::Value,
        }
    }

    /// Evaluates every combinator against the network state collected earlier
    /// this tick, then commits the results in one pass.
    ///
    /// The two phases are separate so no combinator can observe another's
    /// freshly written output: results only become visible when
    /// [`Simulation::collect_circuit_sources`] republishes them next tick.
    pub(in crate::simulation) fn advance_combinators(&mut self) {
        let mut pending = std::mem::take(&mut self.circuits.pending_outputs);
        pending.clear();
        let mut inputs = std::mem::take(&mut self.circuits.evaluation_scratch);

        for (&entity_id, state) in &self.entities.arithmetic_combinators {
            let node = CircuitNode::new(entity_id, ConnectorPort::Input);
            self.circuits.merged_at(node, &mut inputs);
            let mut outputs = SignalSet::default();
            self.evaluate_arithmetic(state, &inputs, &mut outputs);
            pending.push((entity_id, outputs.as_slice().to_vec()));
        }
        for (&entity_id, state) in &self.entities.decider_combinators {
            let node = CircuitNode::new(entity_id, ConnectorPort::Input);
            self.circuits.merged_at(node, &mut inputs);
            let mut outputs = SignalSet::default();
            self.evaluate_decider(state, &inputs, &mut outputs);
            pending.push((entity_id, outputs.as_slice().to_vec()));
        }

        for (entity_id, outputs) in pending.drain(..) {
            if let Some(state) = self.entities.arithmetic_combinators.get_mut(&entity_id) {
                state.outputs = outputs;
            } else if let Some(state) = self.entities.decider_combinators.get_mut(&entity_id) {
                state.outputs = outputs;
            }
        }

        self.circuits.evaluation_scratch = inputs;
        self.circuits.pending_outputs = pending;
    }

    fn evaluate_arithmetic(
        &self,
        state: &ArithmeticCombinatorState,
        inputs: &SignalSet,
        outputs: &mut SignalSet,
    ) {
        let Some(output) = state.output else {
            return;
        };
        let output_is_each = self.signal_role(output) == SignalRole::Each;

        // `Each` on the left runs the operation once per input signal; the
        // result either fans back out per signal (`Each` output) or is summed
        // onto one channel.
        if self.operand_role(state.left) == SignalRole::Each {
            let Some(right) = self.operand_value(state.right, inputs) else {
                return;
            };
            for (signal, value) in inputs.iter() {
                let result = state.operation.apply(value, right);
                let target = if output_is_each { signal } else { output };
                outputs.add(target, result);
            }
            return;
        }

        // Without `Each` on the left there is nothing to fan out, so an `Each`
        // output has no meaning and the combinator stays silent.
        if output_is_each {
            return;
        }
        let (Some(left), Some(right)) = (
            self.operand_value(state.left, inputs),
            self.operand_value(state.right, inputs),
        ) else {
            return;
        };
        outputs.add(output, state.operation.apply(left, right));
    }

    fn evaluate_decider(
        &self,
        state: &DeciderCombinatorState,
        inputs: &SignalSet,
        outputs: &mut SignalSet,
    ) {
        let (Some(left), Some(output)) = (state.left, state.output) else {
            return;
        };
        let Some(right) = self.operand_value(state.right, inputs) else {
            return;
        };
        let output_is_each = self.signal_role(output) == SignalRole::Each;
        let emitted = |signal: SignalId| match state.output_value {
            DeciderOutputValue::One => 1,
            DeciderOutputValue::InputCount => inputs.value(signal),
        };

        match self.signal_role(left) {
            SignalRole::Each => {
                for (signal, value) in inputs.iter() {
                    if !state.comparator.apply(value, right) {
                        continue;
                    }
                    if output_is_each {
                        outputs.add(signal, emitted(signal));
                    } else {
                        // A fixed output collects one contribution per passing
                        // signal, so `InputCount` sums their input values.
                        let amount = match state.output_value {
                            DeciderOutputValue::One => 1,
                            DeciderOutputValue::InputCount => value,
                        };
                        outputs.add(output, amount);
                    }
                }
            }
            role => {
                if output_is_each {
                    return;
                }
                let passes = match role {
                    SignalRole::Anything => inputs
                        .iter()
                        .any(|(_, value)| state.comparator.apply(value, right)),
                    // Vacuously true with no input signals, matching how an
                    // empty network reads as all-zero everywhere else.
                    SignalRole::Everything => inputs
                        .iter()
                        .all(|(_, value)| state.comparator.apply(value, right)),
                    SignalRole::Value | SignalRole::Each => {
                        state.comparator.apply(inputs.value(left), right)
                    }
                };
                if passes {
                    outputs.add(output, emitted(output));
                }
            }
        }
    }

    fn operand_role(&self, operand: SignalOperand) -> SignalRole {
        match operand {
            SignalOperand::Signal(signal) => self.signal_role(signal),
            SignalOperand::Constant(_) => SignalRole::Value,
        }
    }

    /// Resolves an operand to a number. Wildcards have no single value on the
    /// right-hand side, so they resolve to `None` and silence the combinator
    /// rather than being silently read as zero.
    fn operand_value(&self, operand: SignalOperand, inputs: &SignalSet) -> Option<i32> {
        match operand {
            SignalOperand::Constant(value) => Some(value),
            SignalOperand::Signal(signal) => match self.signal_role(signal) {
                SignalRole::Value => Some(inputs.value(signal)),
                _ => None,
            },
        }
    }

    /// Whether `condition` holds for the signals reaching `node`.
    pub(in crate::simulation) fn condition_holds_at(
        &self,
        node: CircuitNode,
        condition: CircuitCondition,
        scratch: &mut SignalSet,
    ) -> bool {
        let right = match condition.right {
            SignalOperand::Constant(value) => value,
            SignalOperand::Signal(signal) => match self.signal_role(signal) {
                SignalRole::Value => self.circuits.value_at(node, signal),
                // A wildcard has no single value to compare against.
                _ => return false,
            },
        };

        match self.signal_role(condition.left) {
            SignalRole::Value => condition
                .comparator
                .apply(self.circuits.value_at(node, condition.left), right),
            SignalRole::Anything => {
                self.circuits.merged_at(node, scratch);
                scratch
                    .iter()
                    .any(|(_, value)| condition.comparator.apply(value, right))
            }
            SignalRole::Everything => {
                self.circuits.merged_at(node, scratch);
                scratch
                    .iter()
                    .all(|(_, value)| condition.comparator.apply(value, right))
            }
            // `Each` selects a per-signal iteration that a single on/off
            // decision cannot express, so it never enables the entity.
            SignalRole::Each => false,
        }
    }
}
