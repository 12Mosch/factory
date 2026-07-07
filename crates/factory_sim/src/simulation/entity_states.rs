use super::validation::machines::{
    validate_assembler, validate_belt_segment, validate_boiler, validate_burner_mining_drill,
    validate_furnace, validate_inserter, validate_lab, validate_splitter_state,
};
use super::*;

/// Per-kind behavior of an entity state map value. Every state type listed in
/// `for_each_entity_state_map!` must implement this; registry-generated code
/// dispatches through it for destroy recovery and save validation.
pub(crate) trait EntityStateBehavior {
    /// Items handed back to the player when the owning entity is destroyed.
    fn push_recovery_stacks(&self, _stacks: &mut Vec<ItemStack>) {}

    /// Validates the state against the catalog and simulation invariants.
    fn validate_state(
        &self,
        _sim: &Simulation,
        _entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        Ok(())
    }
}

impl EntityStateBehavior for Inventory {
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
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
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
        push_optional_stack(stacks, self.energy.fuel_slot);
        push_optional_stack(stacks, self.output_slot);
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
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
        push_optional_stack(stacks, self.input_slot);
        push_optional_stack(stacks, self.energy.fuel_slot);
        push_optional_stack(stacks, self.output_slot);
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
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
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
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
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

impl EntityStateBehavior for ElectricPoleState {}

impl EntityStateBehavior for ElectricConsumerState {
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

impl EntityStateBehavior for SteamEngineState {}

impl EntityStateBehavior for BoilerState {
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
        push_optional_stack(stacks, self.energy.fuel_slot);
    }

    fn validate_state(
        &self,
        sim: &Simulation,
        entity_id: EntityId,
    ) -> Result<(), SimValidationError> {
        validate_boiler(sim, entity_id, self)
    }
}

impl EntityStateBehavior for OffshorePumpState {}

// Fluid box contents are validated network-wide by `validate_fluid_box_states`
// and hold no recoverable items.
impl EntityStateBehavior for Vec<FluidBoxState> {}

impl EntityStateBehavior for BeltSegment {
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
        stacks.extend(self.lanes.iter().flat_map(|lane| {
            lane.items.iter().map(|item| ItemStack {
                item_id: item.item_id,
                count: 1,
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
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
        stacks.extend(self.input_lanes.iter().flat_map(|input_lanes| {
            input_lanes.iter().flat_map(|lane| {
                lane.items.iter().map(|item| ItemStack {
                    item_id: item.item_id,
                    count: 1,
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
    fn push_recovery_stacks(&self, stacks: &mut Vec<ItemStack>) {
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

fn push_inventory_stacks(stacks: &mut Vec<ItemStack>, inventory: &Inventory) {
    stacks.extend(inventory.slots.iter().flatten().copied());
}

fn push_optional_stack(stacks: &mut Vec<ItemStack>, stack: Option<ItemStack>) {
    if let Some(stack) = stack {
        stacks.push(stack);
    }
}
