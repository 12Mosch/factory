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
            && self.energy_remaining_joules == other.energy_remaining_joules
            && self.energy_usage_watts == other.energy_usage_watts
    }
}

impl Hash for BurnerEnergy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fuel_slot.hash(state);
        hash_float(self.energy_remaining_joules, state);
        hash_float(self.energy_usage_watts, state);
    }
}

fn hash_float<H: Hasher>(value: f64, state: &mut H) {
    let bits = if value == 0.0 {
        0.0f64.to_bits()
    } else {
        value.to_bits()
    };
    bits.hash(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn burner_energy_equality_and_hash_treat_signed_zero_as_equal() {
        let positive_zero = BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: 0.0,
            energy_usage_watts: -0.0,
        };
        let negative_zero = BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: -0.0,
            energy_usage_watts: 0.0,
        };

        assert_eq!(positive_zero, negative_zero);
        assert_eq!(hash(&positive_zero), hash(&negative_zero));
    }

    #[test]
    fn burner_energy_equality_uses_float_nan_semantics() {
        let nan_energy = BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: f64::NAN,
            energy_usage_watts: 0.0,
        };
        let other_nan_energy = BurnerEnergy {
            fuel_slot: None,
            energy_remaining_joules: f64::NAN,
            energy_usage_watts: 0.0,
        };

        assert_ne!(nan_energy, other_nan_energy);
    }

    fn hash(energy: &BurnerEnergy) -> u64 {
        let mut hasher = DefaultHasher::new();
        energy.hash(&mut hasher);
        hasher.finish()
    }
}
