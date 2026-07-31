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
                    state.output_signal,
                );
            }
        }

        self.publish_stopped_train_cargo(&mut networks);
        self.publish_constant_combinators(&mut networks);
        self.publish_stored_combinator_outputs(&mut networks);

        self.circuits.networks = networks;
    }

    /// Publishes what each stopped train is carrying onto the stop it is
    /// standing at.
    ///
    /// Driven from the *trains* rather than from the stops, which is the
    /// difference between one walk over the world's trains per tick and one per
    /// wired stop. A train knows which stop it claimed, so the stop it publishes
    /// to is a lookup; asked the other way round — "which train is standing
    /// here?" — every stop would have to search every train, and a railway with
    /// twenty wired stations and twenty trains would pay four hundred times over
    /// for an answer twenty walks could give.
    ///
    /// Only a train that has *arrived* counts. One still on its way has claimed
    /// this platform but is nowhere near it, and publishing its cargo would have
    /// a station reporting goods that are still out on the main line.
    ///
    /// Two trains standing at one stop — which a train limit above one allows —
    /// report the sum of what they carry, because that is what is standing
    /// there. Nothing here picks one of them, so no answer depends on which
    /// train happened to arrive first.
    fn publish_stopped_train_cargo(&self, networks: &mut [SignalSet]) {
        if self.entities.train_stops.is_empty() {
            return;
        }
        for train in self.rolling_stock.trains() {
            // Asked of the train through the same predicate the schedule uses,
            // so what counts as standing at a stop cannot come to mean one
            // thing to the wait conditions and another to the wires.
            let Some(stop_id) = train
                .scheduled_stop
                .filter(|_| train.is_waiting_at_scheduled_stop())
            else {
                continue;
            };
            // The same two gates the loop above applies: the prototype has to
            // offer a contents reading, and the player has to have asked for it.
            if !self
                .entities
                .circuit_entities
                .get(&stop_id)
                .is_some_and(|state| state.read_contents)
                || !self
                    .entities
                    .placed_entity(stop_id)
                    .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
                    .and_then(|prototype| prototype.circuit_connector)
                    .is_some_and(|connector| connector.reads_contents)
            {
                continue;
            }
            let node = CircuitNode::new(stop_id, ConnectorPort::Single);
            let cargo = self.train_cargo(train.id);
            for (&item_id, &count) in &cargo.items {
                self.publish(networks, node, SignalId::Item(item_id), count);
            }
            for (&fluid_id, &milliunits) in &cargo.fluids {
                let units = u64::try_from(milliunits).unwrap_or(0) / FLUID_MILLIUNITS_PER_UNIT;
                self.publish(
                    networks,
                    node,
                    SignalId::Fluid(fluid_id),
                    signal_value_from_count(units),
                );
            }
        }
    }

    /// Adds one entity's readable contents onto the networks at `node`.
    fn publish_entity_contents(
        &self,
        networks: &mut [SignalSet],
        node: CircuitNode,
        entity_id: EntityId,
        prototype: &factory_data::EntityPrototype,
        output_signal: Option<SignalId>,
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
            // A signal reports its aspect, on a channel the player picks for the
            // same reason an accumulator's charge needs one: there is no item or
            // fluid the reading naturally belongs to. Circuits resolve before the
            // rails do, so the aspect a network carries is the one the previous
            // tick's reservation settled — the same one-tick delay a combinator
            // and a roboport's network contents both have.
            EntityKind::RailSignal | EntityKind::ChainSignal => {
                let Some(signal) = output_signal else {
                    return;
                };
                let Some(aspect) = self.rail_signal_aspect(entity_id) else {
                    return;
                };
                self.publish(networks, node, signal, aspect.circuit_value());
            }
            EntityKind::Accumulator => {
                // Accumulators report charge as a percentage, which is the
                // one reading that has no natural item or fluid channel of its
                // own, so the player picks the signal it lands on.
                let Some(signal) = output_signal else {
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
            | EntityKind::StorageTank
            // Heat exchangers hold water and steam boxes like a boiler does.
            | EntityKind::HeatExchanger => {
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
            // A roboport reports the logistic contents of the network it
            // anchors rather than its own two inventories: the network is the
            // interesting quantity, and the roboport is the one entity that
            // already knows which network it belongs to. The figures come from
            // the logistic index, which the robot pass refreshes later in the
            // tick, so a connector reads the network as it stood at the end of
            // the previous tick — the same one-tick delay every combinator has.
            EntityKind::Roboport => {
                let Some(contents) = self
                    .robot_network_id_for_entity(entity_id)
                    .and_then(|network_id| self.logistic_network_contents(network_id))
                else {
                    return;
                };
                for (&item_id, totals) in contents {
                    self.publish(
                        networks,
                        node,
                        SignalId::Item(item_id),
                        signal_value_from_count(u64::from(totals.stored)),
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
            | EntityKind::DeciderCombinator
            // Reactors and heat pipes carry temperature rather than a signal
            // the circuit network can express as an item or fluid count.
            | EntityKind::NuclearReactor
            | EntityKind::HeatPipe
            // Track holds nothing and does no work; what a rail network could
            // report is the rolling stock standing on it, and that belongs to
            // the station a wire would actually be attached to.
            | EntityKind::RailStraight
            | EntityKind::RailCurved
            // Rolling stock is never a placed entity, so a wire could not reach
            // one of these prototypes in the first place: a wagon's contents are
            // read at the station it is stopped in, not off the wagon.
            | EntityKind::Locomotive
            | EntityKind::CargoWagon
            | EntityKind::FluidWagon
            // A stop does report what is standing at it, but it is published by
            // `publish_stopped_train_cargo` rather than here: the question is
            // one the *trains* answer cheaply and the stops answer by searching.
            | EntityKind::TrainStop => {}
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
