use super::validation::machines::{
    validate_assembler, validate_belt_segment, validate_boiler, validate_burner_mining_drill,
    validate_furnace, validate_inserter, validate_lab, validate_splitter_state,
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

impl EntityStateBehavior for BurnerMiningDrillState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_item_slot(stacks, self.energy.fuel_slot);
        push_item_slot(stacks, self.output_slot);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_burner_mining_drill(sim, entity_id, self)
    }
}

impl EntityStateBehavior for FurnaceState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_item_slot(stacks, self.input_slot);
        push_item_slot(stacks, self.energy.fuel_slot);
        push_item_slot(stacks, self.output_slot);
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
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_assembler(sim, entity_id, self)
    }
}

impl EntityStateBehavior for LabState {
    fn push_recovery_stacks(&self, _catalog: &PrototypeCatalog, stacks: &mut Vec<ItemStack>) {
        push_inventory_stacks(stacks, &self.inventory);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_lab(sim, entity_id, self)
    }
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

fn push_inventory_stacks(stacks: &mut Vec<ItemStack>, inventory: &Inventory) {
    stacks.extend(inventory.slots().iter().filter_map(|slot| slot.stack()));
}

fn push_item_slot(stacks: &mut Vec<ItemStack>, slot: ItemSlot) {
    if let Some(stack) = slot.stack() {
        stacks.push(stack);
    }
}
