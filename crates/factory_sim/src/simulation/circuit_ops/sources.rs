use super::*;

/// One fluid unit is a thousand milliunits; networks report whole units so a
/// tank reads the same number the fluid UI shows.
const FLUID_MILLIUNITS_PER_UNIT: u64 = 1_000;

impl Simulation {
    /// Fills every network with this tick's source values: entity contents,
    /// constant combinators, and the combinator outputs computed last tick.
    pub(in crate::simulation) fn collect_circuit_sources(&mut self) {
        let mut networks = std::mem::take(&mut self.circuits.networks);
        for network in &mut networks {
            network.clear();
        }

        for (&entity_id, state) in &self.entities.circuit_entities {
            if state.connections.is_empty() {
                continue;
            }
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(connector) = prototype.circuit_connector else {
                continue;
            };

            if connector.reads_contents && state.read_contents {
                let node = CircuitNode::new(entity_id, ConnectorPort::Single);
                self.publish_entity_contents(
                    &mut networks,
                    node,
                    entity_id,
                    prototype,
                    state.charge_output_signal,
                );
            }
        }

        self.publish_constant_combinators(&mut networks);
        self.publish_stored_combinator_outputs(&mut networks);

        self.circuits.networks = networks;
    }

    /// Adds one entity's readable contents onto the networks at `node`.
    fn publish_entity_contents(
        &self,
        networks: &mut [SignalSet],
        node: CircuitNode,
        entity_id: EntityId,
        prototype: &factory_data::EntityPrototype,
        charge_output_signal: Option<SignalId>,
    ) {
        match prototype.entity_kind {
            EntityKind::Chest => {
                let Some(inventory) = self.entities.entity_inventories.get(&entity_id) else {
                    return;
                };
                for slot in inventory.slots() {
                    if let Some(stack) = slot.stack() {
                        self.publish(
                            networks,
                            node,
                            SignalId::Item(stack.item_id()),
                            i32::from(stack.count()),
                        );
                    }
                }
            }
            EntityKind::TransportBelt => {
                let Some(segment) = self.entities.transport_belts.get(&entity_id) else {
                    return;
                };
                for lane in &segment.lanes {
                    for item in &lane.items {
                        self.publish(networks, node, SignalId::Item(item.item_id), 1);
                    }
                }
            }
            EntityKind::Accumulator => {
                // Accumulators report charge as a percentage, which is the
                // one reading that has no natural item or fluid channel of its
                // own, so the player picks the signal it lands on.
                let Some(signal) = charge_output_signal else {
                    return;
                };
                let Some(state) = self.entities.accumulators.get(&entity_id) else {
                    return;
                };
                let Some(capacity) = prototype
                    .accumulator
                    .map(|accumulator| accumulator.capacity_joules)
                    .filter(|capacity| *capacity > 0)
                else {
                    return;
                };
                let percent = state.stored_energy_joules.saturating_mul(100) / capacity;
                self.publish(networks, node, signal, signal_value_from_count(percent));
            }
            EntityKind::Furnace
            | EntityKind::MiningDrill
            | EntityKind::AssemblingMachine
            | EntityKind::SteamEngine
            | EntityKind::Boiler
            | EntityKind::OffshorePump
            | EntityKind::Pump
            | EntityKind::Pumpjack
            | EntityKind::Pipe
            | EntityKind::StorageTank => {
                let Some(boxes) = self.entities.fluid_boxes.get(&entity_id) else {
                    return;
                };
                for fluid_box in boxes {
                    let Some(fluid_id) = fluid_box.fluid_id else {
                        continue;
                    };
                    let units = fluid_box.amount_milliunits / FLUID_MILLIUNITS_PER_UNIT;
                    self.publish(
                        networks,
                        node,
                        SignalId::Fluid(fluid_id),
                        signal_value_from_count(units),
                    );
                }
            }
            // These entity kinds have no implemented circuit-readable source.
            // Keeping this match exhaustive makes a newly added kind require
            // an explicit decision here.
            EntityKind::ResourcePatch
            | EntityKind::Inserter
            | EntityKind::Splitter
            | EntityKind::Lab
            | EntityKind::Beacon
            | EntityKind::ElectricPole
            | EntityKind::Wall
            | EntityKind::GunTurret
            | EntityKind::LaserTurret
            | EntityKind::EnemySpawner
            | EntityKind::SolarPanel
            | EntityKind::Radar
            | EntityKind::Lamp
            | EntityKind::ConstantCombinator
            | EntityKind::ArithmeticCombinator
            | EntityKind::DeciderCombinator => {}
        }
    }

    fn publish_constant_combinators(&self, networks: &mut [SignalSet]) {
        for (&entity_id, state) in &self.entities.constant_combinators {
            if !state.enabled {
                continue;
            }
            let node = CircuitNode::new(entity_id, ConnectorPort::Output);
            for slot in &state.slots {
                if let Some(signal) = slot.signal {
                    self.publish(networks, node, signal, slot.value);
                }
            }
        }
    }

    /// Publishes the results the arithmetic and decider combinators computed
    /// during the previous tick. Reading stored outputs here — rather than
    /// evaluating inline — is what gives every combinator the same one-tick
    /// delay and keeps chains free of same-tick cascades.
    fn publish_stored_combinator_outputs(&self, networks: &mut [SignalSet]) {
        for (&entity_id, state) in &self.entities.arithmetic_combinators {
            let node = CircuitNode::new(entity_id, ConnectorPort::Output);
            for &(signal, value) in &state.outputs {
                self.publish(networks, node, signal, value);
            }
        }
        for (&entity_id, state) in &self.entities.decider_combinators {
            let node = CircuitNode::new(entity_id, ConnectorPort::Output);
            for &(signal, value) in &state.outputs {
                self.publish(networks, node, signal, value);
            }
        }
    }

    /// Adds `value` to every network `node` is wired to. A connector wired
    /// with both colors publishes the same value onto both.
    fn publish(&self, networks: &mut [SignalSet], node: CircuitNode, signal: SignalId, value: i32) {
        for color in WireColor::ALL {
            let Some(network_id) = self.circuits.topology.network_id(node, color) else {
                continue;
            };
            if let Some(network) = networks.get_mut(network_id as usize) {
                network.add(signal, value);
            }
        }
    }
}
