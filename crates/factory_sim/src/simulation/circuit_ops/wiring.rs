use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitError {
    MissingEntity(EntityId),
    /// The entity's prototype declares no circuit connector.
    NotConnectable(EntityId),
    /// The requested port does not exist on that entity's connector layout.
    InvalidPort {
        entity_id: EntityId,
        port: ConnectorPort,
    },
    /// Both endpoints are the same connector.
    SelfConnection(EntityId),
    OutOfReach {
        first: EntityId,
        second: EntityId,
    },
    AlreadyConnected,
    NotConnected,
    /// No wire item of the requested color in the player inventory.
    MissingWireItem(ItemId),
    /// The entity does not support an enable/disable condition.
    NotControllable(EntityId),
    /// The entity does not publish its contents onto a network.
    DoesNotReadContents(EntityId),
    /// The entity has no single scalar reading to publish, so there is no
    /// channel for the player to choose.
    NoScalarReading(EntityId),
    UnknownSignal(SignalId),
    /// The referenced slot index is outside the combinator's configured rows.
    InvalidSlotIndex {
        entity_id: EntityId,
        slot_index: usize,
    },
    /// The entity is not a combinator of the kind the command configures.
    NotACombinator(EntityId),
}

/// A pair of connectors a wire can join, resolved against the catalog.
struct ResolvedEndpoints {
    first: CircuitNode,
    second: CircuitNode,
}

impl Simulation {
    /// Item consumed and refunded by one wire of `color`.
    pub fn circuit_wire_item(&self, color: WireColor) -> ItemId {
        let name = match color {
            WireColor::Red => "red_wire",
            WireColor::Green => "green_wire",
        };
        factory_data::item_id_by_name(&self.world.prototypes, name)
    }

    /// Joins two connectors with a wire, consuming one wire item.
    pub fn connect_circuit_wire(
        &mut self,
        first: CircuitNode,
        second: CircuitNode,
        color: WireColor,
    ) -> Result<(), CircuitError> {
        let endpoints = self.resolve_endpoints(first, second)?;
        if self
            .entities
            .circuit_entities
            .get(&endpoints.first.entity_id)
            .is_some_and(|state| {
                state
                    .connections
                    .neighbors(endpoints.first.port, color)
                    .contains(&endpoints.second)
            })
        {
            return Err(CircuitError::AlreadyConnected);
        }

        let wire_item = self.circuit_wire_item(color);
        if self.player_inventory.count(wire_item) == 0 {
            return Err(CircuitError::MissingWireItem(wire_item));
        }
        self.player_inventory
            .remove(wire_item, 1)
            .map_err(|_| CircuitError::MissingWireItem(wire_item))?;

        self.circuit_state_mut(endpoints.first.entity_id)
            .connections
            .insert(endpoints.first.port, color, endpoints.second);
        self.circuit_state_mut(endpoints.second.entity_id)
            .connections
            .insert(endpoints.second.port, color, endpoints.first);
        self.invalidate_circuit_topology();
        Ok(())
    }

    /// Cuts a wire, returning its item to the player. A full inventory does
    /// not block the cut; the wire is simply lost, matching how destroyed
    /// entities behave when recovery has nowhere to go.
    pub fn disconnect_circuit_wire(
        &mut self,
        first: CircuitNode,
        second: CircuitNode,
        color: WireColor,
    ) -> Result<(), CircuitError> {
        let endpoints = self.resolve_endpoints(first, second)?;
        let forward_link_exists = self
            .entities
            .circuit_entities
            .get(&endpoints.first.entity_id)
            .is_some_and(|state| {
                state
                    .connections
                    .neighbors(endpoints.first.port, color)
                    .contains(&endpoints.second)
            });
        let back_link_exists = self
            .entities
            .circuit_entities
            .get(&endpoints.second.entity_id)
            .is_some_and(|state| {
                state
                    .connections
                    .neighbors(endpoints.second.port, color)
                    .contains(&endpoints.first)
            });
        if !forward_link_exists || !back_link_exists {
            return Err(CircuitError::NotConnected);
        }
        let removed_forward_link = self
            .entities
            .circuit_entities
            .get_mut(&endpoints.first.entity_id)
            .is_some_and(|state| {
                state
                    .connections
                    .remove(endpoints.first.port, color, endpoints.second)
            });
        let removed_back_link = self
            .entities
            .circuit_entities
            .get_mut(&endpoints.second.entity_id)
            .is_some_and(|state| {
                state
                    .connections
                    .remove(endpoints.second.port, color, endpoints.first)
            });
        debug_assert!(
            removed_forward_link,
            "validated circuit forward link disappeared"
        );
        debug_assert!(removed_back_link, "validated circuit back-link disappeared");
        self.refund_wires(std::iter::once(color));
        self.prune_inert_circuit_state(endpoints.first.entity_id);
        self.prune_inert_circuit_state(endpoints.second.entity_id);
        self.invalidate_circuit_topology();
        Ok(())
    }

    /// Cuts every wire of `color` attached to `entity_id`.
    pub fn disconnect_all_circuit_wires(
        &mut self,
        entity_id: EntityId,
        color: WireColor,
    ) -> Result<usize, CircuitError> {
        if !self.entities.placed_entities.contains_key(&entity_id) {
            return Err(CircuitError::MissingEntity(entity_id));
        }
        let links = self
            .entities
            .circuit_entities
            .get(&entity_id)
            .map(|state| {
                state
                    .connections
                    .iter()
                    .filter(|(_, link_color, _)| *link_color == color)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for &(port, link_color, neighbor) in &links {
            if let Some(state) = self.entities.circuit_entities.get_mut(&entity_id) {
                state.connections.remove(port, link_color, neighbor);
            }
            if let Some(state) = self.entities.circuit_entities.get_mut(&neighbor.entity_id) {
                state.connections.remove(
                    neighbor.port,
                    link_color,
                    CircuitNode::new(entity_id, port),
                );
            }
            self.prune_inert_circuit_state(neighbor.entity_id);
        }
        if links.is_empty() {
            return Ok(0);
        }
        self.refund_wires(links.iter().map(|(_, link_color, _)| *link_color));
        self.prune_inert_circuit_state(entity_id);
        self.invalidate_circuit_topology();
        Ok(links.len())
    }

    /// Clears the reverse links former neighbors hold on `entity_id`, without
    /// refunding anything.
    ///
    /// Called from the destroy paths before the entity's own state map entry
    /// goes away, so a removed entity never leaves dangling wires behind.
    /// Refunds are handled separately by
    /// [`Simulation::circuit_wire_recovery_stacks`], because only the
    /// recovering destroy path returns items to the player at all.
    pub(in crate::simulation) fn unlink_circuit_wires(&mut self, entity_id: EntityId) {
        let Some(state) = self.entities.circuit_entities.get(&entity_id) else {
            return;
        };
        let links = state.connections.iter().collect::<Vec<_>>();
        if links.is_empty() {
            return;
        }
        for &(port, color, neighbor) in &links {
            if let Some(neighbor_state) =
                self.entities.circuit_entities.get_mut(&neighbor.entity_id)
            {
                neighbor_state.connections.remove(
                    neighbor.port,
                    color,
                    CircuitNode::new(entity_id, port),
                );
            }
            self.prune_inert_circuit_state(neighbor.entity_id);
        }
        self.invalidate_circuit_topology();
    }

    /// Wire items recovered when `entity_id` is deconstructed, one per
    /// attached wire.
    pub(in crate::simulation) fn circuit_wire_recovery_stacks(
        &self,
        entity_id: EntityId,
        stacks: &mut Vec<ItemStack>,
    ) {
        let Some(state) = self.entities.circuit_entities.get(&entity_id) else {
            return;
        };
        for (_, color, _) in state.connections.iter() {
            let item_id = self.circuit_wire_item(color);
            if let Ok(stack) = ItemStack::new(&self.world.prototypes, item_id, 1) {
                stacks.push(stack);
            }
        }
    }

    pub fn set_circuit_condition(
        &mut self,
        entity_id: EntityId,
        condition: Option<CircuitCondition>,
    ) -> Result<(), CircuitError> {
        let connector = self.circuit_connector(entity_id)?;
        if !connector.controllable {
            return Err(CircuitError::NotControllable(entity_id));
        }
        if let Some(condition) = condition {
            self.require_known_signal(condition.left)?;
            if let SignalOperand::Signal(signal) = condition.right {
                self.require_known_signal(signal)?;
            }
        }
        self.circuit_state_mut(entity_id).enable_condition = condition;
        self.prune_inert_circuit_state(entity_id);
        Ok(())
    }

    pub fn set_circuit_read_contents(
        &mut self,
        entity_id: EntityId,
        read_contents: bool,
    ) -> Result<(), CircuitError> {
        let connector = self.circuit_connector(entity_id)?;
        if !connector.reads_contents {
            return Err(CircuitError::DoesNotReadContents(entity_id));
        }
        self.circuit_state_mut(entity_id).read_contents = read_contents;
        self.prune_inert_circuit_state(entity_id);
        Ok(())
    }

    /// Picks the channel an entity's one scalar reading is published on: an
    /// accumulator's charge percentage, a rail signal's aspect.
    ///
    /// One command for both rather than one each, because the state, the
    /// validation, and the publishing step are already shared — what differs is
    /// only which number is read at the end of it.
    pub fn set_entity_output_signal(
        &mut self,
        entity_id: EntityId,
        signal: Option<SignalId>,
    ) -> Result<(), CircuitError> {
        self.circuit_connector(entity_id)?;
        if !self.entity_reports_scalar(entity_id) {
            return Err(CircuitError::NoScalarReading(entity_id));
        }
        if let Some(signal) = signal {
            self.require_known_signal(signal)?;
        }
        self.circuit_state_mut(entity_id).output_signal = signal;
        self.prune_inert_circuit_state(entity_id);
        Ok(())
    }

    /// Whether this entity has a single scalar reading a circuit can carry, and
    /// therefore a channel worth choosing.
    ///
    /// Asked of the placed entity rather than of a state map, because a signal
    /// keeps no per-entity state of its own: its aspect is derived every tick
    /// from the reservation.
    pub fn entity_reports_scalar(&self, entity_id: EntityId) -> bool {
        if self.entities.accumulators.contains_key(&entity_id) {
            return true;
        }
        self.entities
            .placed_entity(entity_id)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .is_some_and(|prototype| prototype.entity_kind.is_rail_signal())
    }

    pub fn set_constant_combinator_slot(
        &mut self,
        entity_id: EntityId,
        slot_index: usize,
        slot: ConstantSignalSlot,
    ) -> Result<(), CircuitError> {
        if let Some(signal) = slot.signal {
            self.require_known_signal(signal)?;
        }
        let state = self
            .entities
            .constant_combinators
            .get_mut(&entity_id)
            .ok_or(CircuitError::NotACombinator(entity_id))?;
        let target = state
            .slots
            .get_mut(slot_index)
            .ok_or(CircuitError::InvalidSlotIndex {
                entity_id,
                slot_index,
            })?;
        *target = slot;
        Ok(())
    }

    pub fn set_constant_combinator_enabled(
        &mut self,
        entity_id: EntityId,
        enabled: bool,
    ) -> Result<(), CircuitError> {
        let state = self
            .entities
            .constant_combinators
            .get_mut(&entity_id)
            .ok_or(CircuitError::NotACombinator(entity_id))?;
        state.enabled = enabled;
        Ok(())
    }

    pub fn configure_arithmetic_combinator(
        &mut self,
        entity_id: EntityId,
        left: SignalOperand,
        operation: ArithmeticOperation,
        right: SignalOperand,
        output: Option<SignalId>,
    ) -> Result<(), CircuitError> {
        self.require_known_operand(left)?;
        self.require_known_operand(right)?;
        if let Some(output) = output {
            self.require_known_signal(output)?;
        }
        let state = self
            .entities
            .arithmetic_combinators
            .get_mut(&entity_id)
            .ok_or(CircuitError::NotACombinator(entity_id))?;
        state.left = left;
        state.operation = operation;
        state.right = right;
        state.output = output;
        Ok(())
    }

    pub fn configure_decider_combinator(
        &mut self,
        entity_id: EntityId,
        left: Option<SignalId>,
        comparator: Comparator,
        right: SignalOperand,
        output: Option<SignalId>,
        output_value: DeciderOutputValue,
    ) -> Result<(), CircuitError> {
        if let Some(left) = left {
            self.require_known_signal(left)?;
        }
        self.require_known_operand(right)?;
        if let Some(output) = output {
            self.require_known_signal(output)?;
        }
        let state = self
            .entities
            .decider_combinators
            .get_mut(&entity_id)
            .ok_or(CircuitError::NotACombinator(entity_id))?;
        state.left = left;
        state.comparator = comparator;
        state.right = right;
        state.output = output;
        state.output_value = output_value;
        Ok(())
    }

    /// Validates both endpoints against the catalog and the wire reach shared
    /// by the two connectors.
    fn resolve_endpoints(
        &self,
        first: CircuitNode,
        second: CircuitNode,
    ) -> Result<ResolvedEndpoints, CircuitError> {
        if first == second {
            return Err(CircuitError::SelfConnection(first.entity_id));
        }
        let first_connector = self.circuit_connector(first.entity_id)?;
        let second_connector = self.circuit_connector(second.entity_id)?;
        if !first.port.is_valid_for(first_connector.ports) {
            return Err(CircuitError::InvalidPort {
                entity_id: first.entity_id,
                port: first.port,
            });
        }
        if !second.port.is_valid_for(second_connector.ports) {
            return Err(CircuitError::InvalidPort {
                entity_id: second.entity_id,
                port: second.port,
            });
        }
        // The shorter of the two reaches wins, so a long-reach connector
        // cannot pull a wire further than its partner allows.
        let reach_x2 = i64::from(
            first_connector
                .wire_reach_tiles_x2
                .min(second_connector.wire_reach_tiles_x2),
        );
        if !self.entities_within_wire_reach(first.entity_id, second.entity_id, reach_x2) {
            return Err(CircuitError::OutOfReach {
                first: first.entity_id,
                second: second.entity_id,
            });
        }
        // Two ports of the same combinator are distinct connectors but wiring
        // them together would feed its output straight back into its input.
        if first.entity_id == second.entity_id {
            return Err(CircuitError::SelfConnection(first.entity_id));
        }
        Ok(ResolvedEndpoints { first, second })
    }

    fn entities_within_wire_reach(&self, first: EntityId, second: EntityId, reach_x2: i64) -> bool {
        let (Some(first), Some(second)) = (
            self.entities.placed_entity(first),
            self.entities.placed_entity(second),
        ) else {
            return false;
        };
        centers_within_reach_x2(
            footprint_center_x2(&first.footprint),
            footprint_center_x2(&second.footprint),
            reach_x2,
        )
    }

    pub(in crate::simulation) fn circuit_connector(
        &self,
        entity_id: EntityId,
    ) -> Result<factory_data::CircuitConnectorPrototype, CircuitError> {
        let placed = self
            .entities
            .placed_entity(entity_id)
            .ok_or(CircuitError::MissingEntity(entity_id))?;
        self.world
            .prototypes
            .entity(placed.prototype_id)
            .and_then(|prototype| prototype.circuit_connector)
            .ok_or(CircuitError::NotConnectable(entity_id))
    }

    fn require_known_signal(&self, signal: SignalId) -> Result<(), CircuitError> {
        signal_is_known(&self.world.prototypes, signal)
            .then_some(())
            .ok_or(CircuitError::UnknownSignal(signal))
    }

    fn require_known_operand(&self, operand: SignalOperand) -> Result<(), CircuitError> {
        match operand {
            SignalOperand::Constant(_) => Ok(()),
            SignalOperand::Signal(signal) => self.require_known_signal(signal),
        }
    }

    fn circuit_state_mut(&mut self, entity_id: EntityId) -> &mut CircuitEntityState {
        self.entities.circuit_entities.entry(entity_id).or_default()
    }

    /// Drops the circuit entry once it carries neither wires nor configuration,
    /// so the map only holds entities the player actually touched.
    fn prune_inert_circuit_state(&mut self, entity_id: EntityId) {
        if self
            .entities
            .circuit_entities
            .get(&entity_id)
            .is_some_and(CircuitEntityState::is_inert)
        {
            self.entities.circuit_entities.remove(&entity_id);
        }
    }

    fn refund_wires(&mut self, colors: impl Iterator<Item = WireColor>) {
        let mut red = 0_u16;
        let mut green = 0_u16;
        for color in colors {
            match color {
                WireColor::Red => red = red.saturating_add(1),
                WireColor::Green => green = green.saturating_add(1),
            }
        }
        for (color, count) in [(WireColor::Red, red), (WireColor::Green, green)] {
            if count == 0 {
                continue;
            }
            let item_id = self.circuit_wire_item(color);
            // Overflow is deliberately tolerated: a full inventory loses the
            // wire rather than blocking the disconnect.
            let _ = self
                .player_inventory
                .insert(&self.world.prototypes, item_id, count);
        }
    }
}

pub(in crate::simulation) fn signal_is_known(catalog: &PrototypeCatalog, signal: SignalId) -> bool {
    match signal {
        SignalId::Item(item_id) => catalog.item(item_id).is_some(),
        SignalId::Fluid(fluid_id) => catalog.fluid(fluid_id).is_some(),
        SignalId::Virtual(virtual_id) => catalog.virtual_signal(virtual_id).is_some(),
    }
}
