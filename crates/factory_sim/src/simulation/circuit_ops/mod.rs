mod evaluation;
mod sources;
mod topology;
mod wiring;

use super::*;
use crate::circuits::signal_value_from_count;
use crate::simulation::circuit_state::CircuitTopology;

pub use wiring::CircuitError;

#[allow(unused_imports)]
pub(in crate::simulation) use evaluation::SignalRole;
use wiring::signal_is_known;

impl Simulation {
    /// Resolves this tick's signal networks and combinator outputs.
    ///
    /// Runs before belts, machines, and inserters so a condition configured on
    /// any of them takes effect in the same tick the signals were produced.
    pub(in crate::simulation) fn advance_circuit_networks(&mut self) {
        self.ensure_circuit_topology();
        self.collect_circuit_sources();
        self.refresh_disabled_entities();
        self.advance_combinators();
    }

    /// Re-resolves every enable condition against the network values collected
    /// this tick.
    fn refresh_disabled_entities(&mut self) {
        let mut disabled = std::mem::take(&mut self.circuits.disabled_entities);
        disabled.clear();
        let mut scratch = std::mem::take(&mut self.circuits.evaluation_scratch);
        // `circuit_entities` is a `BTreeMap`, so this collects in ascending id
        // order and stays binary-searchable without an extra sort.
        for (&entity_id, state) in &self.entities.circuit_entities {
            let Some(condition) = state.enable_condition else {
                continue;
            };
            let node = CircuitNode::new(entity_id, ConnectorPort::Single);
            // A condition on an unwired entity can never be satisfied, which
            // is the reference behaviour: the entity stays off until it is
            // connected to something.
            let enabled = self.circuits.is_wired(node)
                && self.condition_holds_at(node, condition, &mut scratch);
            if !enabled {
                disabled.push(entity_id);
            }
        }
        debug_assert!(disabled.is_sorted());
        self.circuits.evaluation_scratch = scratch;
        self.circuits.disabled_entities = disabled;
    }

    /// Whether an entity is allowed to work this tick. Entities without a
    /// condition are always allowed, so adding a wire alone never silently
    /// stops a machine.
    pub(in crate::simulation) fn circuit_work_allowed(&self, entity_id: EntityId) -> bool {
        !self.circuits.is_disabled(entity_id)
    }

    /// Recomputes lamp lighting.
    ///
    /// Runs after the power phase rather than with the rest of the circuit
    /// work, because a lamp needs both this tick's signals and this tick's
    /// power satisfaction; reading power from the circuit phase would make
    /// lamps lag a tick behind the network that switched them on.
    pub(in crate::simulation) fn refresh_lamps(&mut self) {
        if self.entities.lamps.is_empty() {
            return;
        }
        let lit_by_entity = self
            .entities
            .lamps
            .keys()
            .map(|&entity_id| (entity_id, self.lamp_should_be_lit(entity_id)))
            .collect::<Vec<_>>();
        for (entity_id, lit) in lit_by_entity {
            if let Some(state) = self.entities.lamps.get_mut(&entity_id) {
                state.lit = lit;
            }
        }
    }

    /// A lamp lights when it is powered and its condition passes. Without a
    /// condition a wired lamp stays lit, which is how an unconditioned lamp
    /// works as a plain powered light.
    fn lamp_should_be_lit(&self, entity_id: EntityId) -> bool {
        let powered = self
            .power
            .entity_statuses
            .get(&entity_id)
            .is_some_and(|status| status.satisfaction_permyriad > 0);
        powered && self.circuit_work_allowed(entity_id)
    }

    /// Signals reaching an entity's shared connector, for presentation.
    pub fn circuit_signals_at_entity(&self, entity_id: EntityId) -> SignalSet {
        let mut signals = SignalSet::default();
        self.circuits.merged_at(
            CircuitNode::new(entity_id, ConnectorPort::Single),
            &mut signals,
        );
        signals
    }

    /// Signals reaching a specific connector, for presentation.
    pub fn circuit_signals_at_node(&self, node: CircuitNode) -> SignalSet {
        let mut signals = SignalSet::default();
        self.circuits.merged_at(node, &mut signals);
        signals
    }

    pub fn circuit_entity_state(&self, entity_id: EntityId) -> Option<&CircuitEntityState> {
        self.entities.circuit_entities.get(&entity_id)
    }

    /// Every wire in the tile rectangle, deduplicated so each wire is reported
    /// once. Used by the renderer to draw connections.
    pub fn circuit_wires_in_tile_rect(
        &self,
        min_x: WorldTileCoord,
        max_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_y: WorldTileCoord,
    ) -> Vec<CircuitWire> {
        if min_x > max_x || min_y > max_y {
            return Vec::new();
        }
        // A wire is drawn whenever either endpoint is on screen, so widen the
        // query by the longest reach any connector declares.
        let reach = (i64::from(self.world.prototypes.max_circuit_wire_reach_tiles_x2()) + 1) / 2;
        let mut wires = Vec::new();
        for entity_id in self.entities.occupancy.entity_ids_in_tile_rect(
            min_x.saturating_sub(reach),
            max_x.saturating_add(reach),
            min_y.saturating_sub(reach),
            max_y.saturating_add(reach),
        ) {
            let Some(state) = self.entities.circuit_entities.get(&entity_id) else {
                continue;
            };
            for (port, color, neighbor) in state.connections.iter() {
                let first = CircuitNode::new(entity_id, port);
                // Both endpoints record the wire; keep only the ordered half.
                if first > neighbor {
                    continue;
                }
                wires.push(CircuitWire {
                    first,
                    second: neighbor,
                    color,
                });
            }
        }
        wires.sort_unstable_by_key(|wire| (wire.first, wire.second, wire.color));
        wires.dedup();
        wires
    }

    pub(in crate::simulation) fn rebuild_circuit_state(&mut self) {
        self.circuits.invalidate_topology();
        self.ensure_circuit_topology();
        self.collect_circuit_sources();
        self.refresh_disabled_entities();
    }
}

/// One drawn circuit connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CircuitWire {
    pub first: CircuitNode,
    pub second: CircuitNode,
    pub color: WireColor,
}

pub(in crate::simulation) fn validate_signal(
    sim: &Simulation,
    entity_id: EntityId,
    signal: SignalId,
) -> Result<(), SimValidationError> {
    signal_is_known(&sim.world.prototypes, signal)
        .then_some(())
        .ok_or(SimValidationError::InvalidCircuitSignal { entity_id })
}

pub(in crate::simulation) fn validate_operand(
    sim: &Simulation,
    entity_id: EntityId,
    operand: SignalOperand,
) -> Result<(), SimValidationError> {
    match operand {
        SignalOperand::Constant(_) => Ok(()),
        SignalOperand::Signal(signal) => validate_signal(sim, entity_id, signal),
    }
}

/// Combinator outputs must be sorted, unique, and free of zero values, which
/// is the canonical form [`SignalSet`] produces. Enforcing it here keeps a
/// tampered save from feeding a non-canonical set back into the networks.
pub(in crate::simulation) fn validate_combinator_outputs(
    sim: &Simulation,
    entity_id: EntityId,
    outputs: &[(SignalId, i32)],
) -> Result<(), SimValidationError> {
    let mut previous: Option<SignalId> = None;
    for &(signal, value) in outputs {
        validate_signal(sim, entity_id, signal)?;
        if value == 0 || previous.is_some_and(|previous| previous >= signal) {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }
        previous = Some(signal);
    }
    Ok(())
}

/// Checks one entity's wires: every prototype involved must be connectable,
/// every port must exist, and every link must be mirrored on the neighbor.
pub(in crate::simulation) fn validate_circuit_entity_state(
    sim: &Simulation,
    entity_id: EntityId,
    state: &CircuitEntityState,
) -> Result<(), SimValidationError> {
    let invalid = || SimValidationError::InvalidEntityState { entity_id };
    let connector = sim
        .circuit_connector(entity_id)
        .map_err(|_| SimValidationError::InvalidEntityState { entity_id })?;

    if state.enable_condition.is_some() && !connector.controllable {
        return Err(invalid());
    }
    if state.read_contents && !connector.reads_contents {
        return Err(invalid());
    }
    if state.charge_output_signal.is_some() && !sim.entities.accumulators.contains_key(&entity_id) {
        return Err(invalid());
    }
    if let Some(condition) = state.enable_condition {
        validate_signal(sim, entity_id, condition.left)?;
        validate_operand(sim, entity_id, condition.right)?;
    }
    if let Some(signal) = state.charge_output_signal {
        validate_signal(sim, entity_id, signal)?;
    }

    for (port, color, neighbor) in state.connections.iter() {
        if !port.is_valid_for(connector.ports) || neighbor.entity_id == entity_id {
            return Err(invalid());
        }
        let neighbor_connector = sim
            .circuit_connector(neighbor.entity_id)
            .map_err(|_| SimValidationError::InvalidEntityState { entity_id })?;
        if !neighbor.port.is_valid_for(neighbor_connector.ports) {
            return Err(invalid());
        }
        let mirrored = sim
            .entities
            .circuit_entities
            .get(&neighbor.entity_id)
            .is_some_and(|neighbor_state| {
                neighbor_state
                    .connections
                    .neighbors(neighbor.port, color)
                    .contains(&CircuitNode::new(entity_id, port))
            });
        if !mirrored {
            return Err(SimValidationError::UnmirroredCircuitWire { entity_id });
        }
    }

    Ok(())
}
