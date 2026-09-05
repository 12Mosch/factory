use super::*;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match *command {
        SimCommand::ConnectCircuitWire {
            first,
            second,
            color,
        } => sim
            .connect_circuit_wire(first, second, color)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::DisconnectCircuitWire {
            first,
            second,
            color,
        } => sim
            .disconnect_circuit_wire(first, second, color)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::DisconnectAllCircuitWires { entity_id, color } => {
            let removed = sim
                .disconnect_all_circuit_wires(entity_id, color)
                .map_err(SimCommandError::Circuit)?;
            return Ok(SimCommandEffect::CircuitWiresRemoved { removed });
        }
        SimCommand::SetCircuitCondition {
            entity_id,
            condition,
        } => sim
            .set_circuit_condition(entity_id, condition)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::SetCircuitReadContents {
            entity_id,
            read_contents,
        } => sim
            .set_circuit_read_contents(entity_id, read_contents)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::SetEntityOutputSignal { entity_id, signal } => sim
            .set_entity_output_signal(entity_id, signal)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::SetConstantCombinatorSlot {
            entity_id,
            slot_index,
            slot,
        } => sim
            .set_constant_combinator_slot(entity_id, slot_index, slot)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::SetConstantCombinatorEnabled { entity_id, enabled } => sim
            .set_constant_combinator_enabled(entity_id, enabled)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::ConfigureArithmeticCombinator {
            entity_id,
            left,
            operation,
            right,
            output,
        } => sim
            .configure_arithmetic_combinator(entity_id, left, operation, right, output)
            .map_err(SimCommandError::Circuit)?,
        SimCommand::ConfigureDeciderCombinator {
            entity_id,
            left,
            comparator,
            right,
            output,
            output_value,
        } => sim
            .configure_decider_combinator(entity_id, left, comparator, right, output, output_value)
            .map_err(SimCommandError::Circuit)?,
        _ => unreachable!("non-circuit command routed to circuit dispatcher"),
    }
    Ok(SimCommandEffect::None)
}
