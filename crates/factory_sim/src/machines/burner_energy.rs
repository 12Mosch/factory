use crate::inventory::ItemStack;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BurnerEnergy {
    pub fuel_slot: Option<ItemStack>,
    pub energy_remaining_joules: f64,
    pub energy_usage_watts: f64,
}

impl PartialEq for BurnerEnergy {
    fn eq(&self, other: &Self) -> bool {
        self.fuel_slot == other.fuel_slot
            && self.energy_remaining_joules.to_bits() == other.energy_remaining_joules.to_bits()
            && self.energy_usage_watts.to_bits() == other.energy_usage_watts.to_bits()
    }
}

impl Eq for BurnerEnergy {}

impl Hash for BurnerEnergy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fuel_slot.hash(state);
        self.energy_remaining_joules.to_bits().hash(state);
        self.energy_usage_watts.to_bits().hash(state);
    }
}
