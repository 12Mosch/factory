use crate::ids::EntityId;
use crate::inventory::{InventoryError, ItemSlot, ItemStack};
use factory_data::{ItemId, ModuleEffectPrototype, PrototypeCatalog};
use serde::{Deserialize, Serialize};

pub const PERMYRIAD: i64 = 10_000;
pub const MIN_MULTIPLIER_PERMYRIAD: i64 = 2_000;

/// A fixed-length collection of single-item module slots.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ModuleSlots {
    slots: Vec<ItemSlot>,
}

impl ModuleSlots {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: vec![ItemSlot::default(); slot_count],
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn slot(&self, slot_index: usize) -> Option<ItemStack> {
        self.slots.get(slot_index).and_then(|slot| slot.stack())
    }

    pub fn slots(&self) -> &[ItemSlot] {
        &self.slots
    }

    pub(crate) fn slot_mut(&mut self, slot_index: usize) -> Option<&mut ItemSlot> {
        self.slots.get_mut(slot_index)
    }

    pub fn validate(&self, catalog: &PrototypeCatalog) -> Result<(), InventoryError> {
        for slot in &self.slots {
            slot.validate(catalog)?;
            if slot.stack().is_some_and(|stack| {
                stack.count() != 1
                    || catalog
                        .item(stack.item_id())
                        .is_none_or(|item| item.module_effect.is_none())
            }) {
                return Err(InventoryError::StackExceedsLimit {
                    item_id: slot.stack().map_or(ItemId::new(0), |stack| stack.item_id()),
                    count: slot.stack().map_or(0, |stack| stack.count()),
                    stack_size: 1,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResolvedModuleEffects {
    pub speed_delta_permyriad: i64,
    pub productivity_permyriad: u64,
    pub energy_delta_permyriad: i64,
    pub pollution_delta_permyriad: i64,
}

impl ResolvedModuleEffects {
    pub fn add_effect(&mut self, effect: ModuleEffectPrototype, strength_permyriad: u16) {
        let strength = i64::from(strength_permyriad);
        self.speed_delta_permyriad = self.speed_delta_permyriad.saturating_add(
            i64::from(effect.speed_delta_permyriad).saturating_mul(strength) / PERMYRIAD,
        );
        self.energy_delta_permyriad = self.energy_delta_permyriad.saturating_add(
            i64::from(effect.energy_delta_permyriad).saturating_mul(strength) / PERMYRIAD,
        );
        self.pollution_delta_permyriad = self.pollution_delta_permyriad.saturating_add(
            i64::from(effect.pollution_delta_permyriad).saturating_mul(strength) / PERMYRIAD,
        );
        self.productivity_permyriad = self.productivity_permyriad.saturating_add(
            u64::from(effect.productivity_permyriad).saturating_mul(u64::from(strength_permyriad))
                / PERMYRIAD as u64,
        );
    }

    pub fn speed_multiplier_permyriad(self) -> u64 {
        multiplier(self.speed_delta_permyriad)
    }

    pub fn energy_multiplier_permyriad(self) -> u64 {
        multiplier(self.energy_delta_permyriad)
    }

    pub fn explicit_pollution_multiplier_permyriad(self) -> u64 {
        multiplier(self.pollution_delta_permyriad)
    }

    pub fn pollution_multiplier_permyriad(self) -> u64 {
        self.energy_multiplier_permyriad()
            .saturating_mul(self.explicit_pollution_multiplier_permyriad())
            .checked_div(PERMYRIAD as u64)
            .unwrap_or(u64::MAX)
            .max(MIN_MULTIPLIER_PERMYRIAD as u64)
    }

    pub fn productivity_permyriad(self) -> u64 {
        self.productivity_permyriad
    }
}

fn multiplier(delta: i64) -> u64 {
    PERMYRIAD
        .saturating_add(delta)
        .max(MIN_MULTIPLIER_PERMYRIAD) as u64
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MachineModuleState {
    pub slots: ModuleSlots,
    pub resolved_effects: ResolvedModuleEffects,
    pub productivity_progress_permyriad: u32,
}

impl MachineModuleState {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: ModuleSlots::with_slot_count(slot_count),
            resolved_effects: ResolvedModuleEffects::default(),
            productivity_progress_permyriad: 0,
        }
    }

    /// Returns the number of output copies due for the next successful cycle.
    pub fn output_copies_due(&self) -> u64 {
        1 + (u64::from(self.productivity_progress_permyriad)
            .saturating_add(self.resolved_effects.productivity_permyriad())
            / PERMYRIAD as u64)
    }

    /// Commits productivity after a successful cycle and returns bonus copies.
    pub fn complete_productive_cycle(&mut self) -> u64 {
        let accumulated = u64::from(self.productivity_progress_permyriad)
            .saturating_add(self.resolved_effects.productivity_permyriad());
        self.productivity_progress_permyriad = (accumulated % PERMYRIAD as u64) as u32;
        accumulated / PERMYRIAD as u64
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BeaconState {
    pub slots: ModuleSlots,
}

impl BeaconState {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: ModuleSlots::with_slot_count(slot_count),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleError {
    MissingEntity(EntityId),
    UnsupportedMachine(EntityId),
    InvalidModule(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
}
