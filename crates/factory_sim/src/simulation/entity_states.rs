use super::validation::machines::{
    validate_assembler, validate_belt_segment, validate_boiler, validate_furnace,
    validate_inserter, validate_lab, validate_mining_drill, validate_rocket_silo,
    validate_splitter_state,
};
use super::*;

/// Per-kind behavior of an entity state map value. Every state type listed in
/// `for_each_entity_state_map!` must implement this; registry-generated code
/// dispatches through it for destroy recovery and save validation.
///
/// Both methods are deliberately required: a state type that holds no items
/// or needs no validation must say so with an explicit no-op body instead of
/// silently inheriting one.
pub(crate) trait EntityStateBehavior {
    /// Items handed back to the player when the owning entity is destroyed.
    fn push_recovery_stacks(&self, catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>);

    /// Validates the state against the catalog and simulation invariants.
    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError>;
}

impl EntityStateBehavior for Inventory {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_inventory_stacks(stacks, self);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        super::validation::inventory::validate_inventory(&sim.world.prototypes, self)
    }
}

impl EntityStateBehavior for MiningDrillState {
    fn push_recovery_stacks(&self, catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        if let Some(fuel_slot) = self.energy.fuel_slot() {
            push_item_slot(stacks, fuel_slot);
        }
        push_item_slot(stacks, self.output_slot);
        if let Some(pending) = self.pending_output {
            push_item_count_stacks(catalog, stacks, pending.item_id, pending.count);
        }
        push_module_stacks(stacks, &self.modules.slots);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_mining_drill(sim, entity_id, self)
    }
}

impl EntityStateBehavior for FurnaceState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_item_slot(stacks, self.input_slot);
        if let Some(fuel_slot) = self.energy.fuel_slot() {
            push_item_slot(stacks, fuel_slot);
        }
        push_item_slot(stacks, self.output_slot);
        push_module_stacks(stacks, &self.modules.slots);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_furnace(sim, entity_id, self)
    }
}

impl EntityStateBehavior for AssemblingMachineState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_inventory_stacks(stacks, &self.input_inventory);
        push_inventory_stacks(stacks, &self.output_inventory);
        push_module_stacks(stacks, &self.modules.slots);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_assembler(sim, entity_id, self)
    }
}

impl EntityStateBehavior for RocketSiloState {
    /// Every slotted item comes back: ingredients, waiting cargo, launch
    /// products, and modules. A part already counted toward the rocket is not
    /// an item and was never in a slot, so mining a half-built silo loses the
    /// rocket rather than refunding it in pieces.
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_inventory_stacks(stacks, &self.input_inventory);
        push_inventory_stacks(stacks, &self.cargo_inventory);
        push_inventory_stacks(stacks, &self.output_inventory);
        push_module_stacks(stacks, &self.modules.slots);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_rocket_silo(sim, entity_id, self)
    }
}

impl EntityStateBehavior for LabState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_inventory_stacks(stacks, &self.inventory);
        push_module_stacks(stacks, &self.modules.slots);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_lab(sim, entity_id, self)
    }
}

impl EntityStateBehavior for BeaconState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_module_stacks(stacks, &self.slots);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let prototype = sim
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .ok_or(SimValidationError::InvalidEntityState { entity_id })?;
        if prototype.entity_kind != EntityKind::Beacon
            || self.slots.len() != prototype.module_slot_count
            || self.slots.validate(&sim.world.prototypes).is_err()
        {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }
        Ok(())
    }
}

fn push_module_stacks(stacks: &mut Vec<ItemStack>, modules: &ModuleSlots) {
    stacks.extend(modules.slots().iter().filter_map(|slot| slot.stack()));
}

impl EntityStateBehavior for ElectricPoleState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for ElectricConsumerState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        if self.work_remainder_permyriad >= POWER_SATISFACTION_FULL_PERMYRIAD {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }

        Ok(())
    }
}

impl EntityStateBehavior for SteamEngineState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for SolarPanelState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for AccumulatorState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let capacity = sim
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.accumulator.as_ref())
            .map(|accumulator| accumulator.capacity_joules)
            .ok_or(SimValidationError::InvalidEntityState { entity_id })?;
        let at_capacity = self.stored_energy_joules == capacity;
        if self.energy_remainder_watt_ticks >= SIMULATION_TICKS_PER_SECOND as u8
            || self.stored_energy_joules > capacity
            || (at_capacity && self.energy_remainder_watt_ticks != 0)
        {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }
        Ok(())
    }
}

impl EntityStateBehavior for RadarState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let metadata = sim
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .filter(|prototype| prototype.entity_kind == EntityKind::Radar)
            .and_then(|prototype| prototype.radar)
            .ok_or(SimValidationError::InvalidEntityState { entity_id })?;
        let candidate_count = crate::radar::far_scan_candidate_count(
            metadata.nearby_reveal_radius_chunks,
            metadata.far_scan_radius_chunks,
        );
        if self.nearby_scan_progress_ticks >= metadata.nearby_scan_interval_ticks
            || self.far_scan_progress_ticks >= metadata.far_scan_interval_ticks
            || self.far_scan_cursor >= candidate_count
        {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }
        Ok(())
    }
}

impl EntityStateBehavior for BoilerState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_item_slot(stacks, self.energy.fuel_slot);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_boiler(sim, entity_id, self)
    }
}

// Heat buffer energy is validated against the prototype capacity by
// `validate_heat_buffer_states` and holds no recoverable items.
impl EntityStateBehavior for HeatBufferState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let capacity = sim
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.heat_buffer.as_ref())
            .map(factory_data::HeatBufferPrototype::capacity_joules)
            .ok_or(SimValidationError::InvalidEntityState { entity_id })?;
        if self.energy_joules > capacity {
            return Err(SimValidationError::InvalidHeatBufferState { entity_id });
        }
        Ok(())
    }
}

impl EntityStateBehavior for NuclearReactorState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_item_slot(stacks, self.energy.fuel_slot);
        push_item_slot(stacks, self.output_slot);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        super::validation::machines::validate_nuclear_reactor(sim, entity_id, self)
    }
}

impl EntityStateBehavior for HeatPipeState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        require_heat_kind(sim, entity_id, EntityKind::HeatPipe)
    }
}

impl EntityStateBehavior for HeatExchangerState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        require_heat_kind(sim, entity_id, EntityKind::HeatExchanger)
    }
}

impl EntityStateBehavior for RoboportState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        // Both inventories come back with the roboport; the charging buffer is
        // energy, not goods, and is simply lost.
        push_inventory_stacks(stacks, &self.robots);
        push_inventory_stacks(stacks, &self.materials);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        super::validation::robots::validate_roboport(sim, entity_id, self)
    }
}

// A logistic chest's items live in the chest inventory the entity already has,
// so the configuration itself contributes nothing to recover.
impl EntityStateBehavior for LogisticChestState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        super::validation::robots::validate_logistic_chest(sim, entity_id, self)
    }
}

// A stop holds a name and a limit, not goods: what comes back when one is
// mined is the stop item itself, which the ordinary build-item recovery
// already hands over.
impl EntityStateBehavior for crate::rolling_stock::TrainStopState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let invalid = || SimValidationError::InvalidEntityState { entity_id };
        sim.entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .filter(|prototype| prototype.entity_kind == EntityKind::TrainStop)
            .ok_or_else(invalid)?;
        // A nameless stop is one no schedule could ask for, and a stop that
        // admits no trains is one no schedule could use: both are states the
        // commands refuse to produce, so a save carrying either has been
        // tampered with.
        if self.name.trim().is_empty() || self.train_limit == 0 {
            return Err(invalid());
        }
        if let Some(signal) = self.train_limit_signal {
            circuit_ops::validate_signal(sim, entity_id, signal)?;
        }
        Ok(())
    }
}

/// Confirms a heat state entry sits on an entity of the expected kind that
/// actually declares a heat buffer, so a stale entry can never look valid.
fn require_heat_kind(
    sim: &Simulation,
    entity_id: EntityId,
    expected: EntityKind,
) -> Result<(), SimValidationError> {
    sim.entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
        .filter(|prototype| prototype.entity_kind == expected && prototype.heat_buffer.is_some())
        .map(|_| ())
        .ok_or(SimValidationError::InvalidEntityState { entity_id })
}

impl EntityStateBehavior for OffshorePumpState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for PumpjackState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

// Fluid box contents are validated network-wide by `validate_fluid_box_states`
// and hold no recoverable items.
impl EntityStateBehavior for Vec<FluidBoxState> {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for BeltSegment {
    fn push_recovery_stacks(&self, catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        stacks.extend(self.lanes.iter().flat_map(|lane| {
            lane.items.iter().map(|item| {
                ItemStack::new(catalog, item.item_id, 1)
                    .expect("validated belt items should have valid stack prototypes")
            })
        }));
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_belt_segment(sim, entity_id, self)
    }
}

impl EntityStateBehavior for SplitterState {
    fn push_recovery_stacks(&self, catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        stacks.extend(self.input_lanes.iter().flat_map(|input_lanes| {
            input_lanes.iter().flat_map(|lane| {
                lane.items.iter().map(|item| {
                    ItemStack::new(catalog, item.item_id, 1)
                        .expect("validated splitter items should have valid stack prototypes")
                })
            })
        }));
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_splitter_state(sim, entity_id, self)
    }
}

impl EntityStateBehavior for InserterState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        if let InserterState::Holding { item } = self {
            stacks.push(*item);
        }
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_inserter(sim, entity_id, self)
    }
}

impl EntityStateBehavior for MachineEnergy {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        if let Some(fuel_slot) = self.fuel_slot() {
            push_item_slot(stacks, fuel_slot);
        }
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        super::validation::machines::validate_inserter_energy(sim, entity_id, self)
    }
}

impl EntityStateBehavior for GunTurretState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        // The opened magazine (`loaded_shots`) is lost; only unopened
        // magazines in the ammo inventory are recovered.
        push_inventory_stacks(stacks, &self.ammo);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        super::validation::inventory::validate_inventory(&sim.world.prototypes, &self.ammo)?;
        for stack in self.ammo.slots().iter().filter_map(|slot| slot.stack()) {
            if !item_slot_policy_accepts(
                &sim.world.prototypes,
                &sim.research,
                &sim.entities,
                ItemSlotPolicy::Ammunition,
                ItemSlotOperation::MachineInsert,
                stack.item_id(),
            ) {
                return Err(SimValidationError::InvalidMachineItem {
                    entity_id,
                    item_id: stack.item_id(),
                });
            }
        }
        if self.loaded_shots > 0 && self.loaded_damage.amount == 0 {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }

        Ok(())
    }
}

impl EntityStateBehavior for LaserTurretState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let cooldown = sim
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.laser_turret)
            .map(|turret| turret.cooldown_ticks)
            .ok_or(SimValidationError::InvalidEntityState { entity_id })?;
        if self.cooldown_remaining_ticks > cooldown
            || (!self.engaged && self.cooldown_remaining_ticks != 0)
        {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }
        Ok(())
    }
}

impl EntityStateBehavior for EnemySpawnerState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for HealthState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let max_health = sim
            .entities
            .placed_entities
            .get(&entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.max_health);
        let Some(max_health) = max_health else {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        };
        let expected_faction = if sim
            .entities
            .placed_entities
            .get(&entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .is_some_and(|prototype| prototype.entity_kind == EntityKind::EnemySpawner)
        {
            Faction::Enemy
        } else {
            Faction::Player
        };
        if self.current == 0
            || self.maximum != max_health
            || self.current > self.maximum
            || self.faction != expected_faction
            || !self.resistances.is_valid()
        {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }

        Ok(())
    }
}

// Wires are refunded by the destroy path itself (it has to walk the neighbor
// entries to unlink them anyway), so the state contributes no recovery stacks
// of its own.
impl EntityStateBehavior for CircuitEntityState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        circuit_ops::validate_circuit_entity_state(sim, entity_id, self)
    }
}

impl EntityStateBehavior for ConstantCombinatorState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        let slot_count = combinator_prototype(sim, entity_id)?.constant_slot_count;
        if self.slots.len() != usize::from(slot_count) {
            return Err(SimValidationError::InvalidEntityState { entity_id });
        }
        for slot in &self.slots {
            if let Some(signal) = slot.signal {
                circuit_ops::validate_signal(sim, entity_id, signal)?;
            }
        }
        Ok(())
    }
}

impl EntityStateBehavior for ArithmeticCombinatorState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        combinator_prototype(sim, entity_id)?;
        circuit_ops::validate_operand(sim, entity_id, self.left)?;
        circuit_ops::validate_operand(sim, entity_id, self.right)?;
        if let Some(output) = self.output {
            circuit_ops::validate_signal(sim, entity_id, output)?;
        }
        circuit_ops::validate_combinator_outputs(sim, entity_id, &self.outputs)
    }
}

impl EntityStateBehavior for DeciderCombinatorState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        combinator_prototype(sim, entity_id)?;
        if let Some(left) = self.left {
            circuit_ops::validate_signal(sim, entity_id, left)?;
        }
        circuit_ops::validate_operand(sim, entity_id, self.right)?;
        if let Some(output) = self.output {
            circuit_ops::validate_signal(sim, entity_id, output)?;
        }
        circuit_ops::validate_combinator_outputs(sim, entity_id, &self.outputs)
    }
}

impl EntityStateBehavior for LampState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, _stacks: &mut Vec<ItemStack>) {}

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        sim.entities
            .placed_entity(entity_id)
            .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
            .filter(|prototype| {
                prototype.entity_kind == EntityKind::Lamp && prototype.circuit_connector.is_some()
            })
            .map(|_| ())
            .ok_or(SimValidationError::InvalidEntityState { entity_id })
    }
}

fn combinator_prototype(
    sim: &Simulation,
    entity_id: EntityId,
) -> Result<factory_data::CombinatorPrototype, SimValidationError> {
    sim.entities
        .placed_entity(entity_id)
        .and_then(|placed| sim.world.prototypes.entity(placed.prototype_id))
        .and_then(|prototype| prototype.combinator)
        .ok_or(SimValidationError::InvalidEntityState { entity_id })
}

fn push_inventory_stacks(stacks: &mut Vec<ItemStack>, inventory: &Inventory) {
    stacks.extend(inventory.slots().iter().filter_map(|slot| slot.stack()));
}

fn push_item_slot(stacks: &mut Vec<ItemStack>, slot: ItemSlot) {
    if let Some(stack) = slot.stack() {
        stacks.push(stack);
    }
}

fn push_item_count_stacks(
    catalog: &PrototypeCatalog,
    stacks: &mut Vec<ItemStack>,
    item_id: ItemId,
    mut count: u64,
) {
    let stack_size = catalog
        .item(item_id)
        .expect("validated recovery item should exist in the catalog")
        .stack_size;
    assert!(
        stack_size > 0,
        "validated recovery item should have a stack size"
    );

    while count > 0 {
        let stack_count = count.min(u64::from(stack_size)) as u16;
        stacks.push(
            ItemStack::new(catalog, item_id, stack_count)
                .expect("bounded recovery item count should form a valid stack"),
        );
        count -= u64::from(stack_count);
    }
}
