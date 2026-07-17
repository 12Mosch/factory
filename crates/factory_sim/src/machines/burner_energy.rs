use crate::inventory::ItemSlot;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Energy source powering a machine that can be either fuel-burning or
/// electric. Burner machines carry their fuel state inline; electric machines
/// hold no per-entity energy state because power satisfaction is tracked by
/// the electric network (`ElectricConsumerState`).
#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub enum MachineEnergy {
    Burner(BurnerEnergy),
    Electric,
}

impl MachineEnergy {
    pub fn burner(&self) -> Option<&BurnerEnergy> {
        match self {
            Self::Burner(burner) => Some(burner),
            Self::Electric => None,
        }
    }

    pub fn burner_mut(&mut self) -> Option<&mut BurnerEnergy> {
        match self {
            Self::Burner(burner) => Some(burner),
            Self::Electric => None,
        }
    }

    pub fn fuel_slot(&self) -> Option<ItemSlot> {
        self.burner().map(|burner| burner.fuel_slot)
    }

    pub fn fuel_slot_mut(&mut self) -> Option<&mut ItemSlot> {
        self.burner_mut().map(|burner| &mut burner.fuel_slot)
    }

    /// Whether a burner is out of both stored energy and fuel; always false
    /// for electric machines.
    pub fn is_out_of_fuel(&self) -> bool {
        self.burner().is_some_and(|burner| {
            burner.fuel_slot.is_empty() && burner.energy_remaining_joules <= f64::EPSILON
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BurnerEnergy {
    pub fuel_slot: ItemSlot,
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
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: 0.0,
            energy_usage_watts: -0.0,
        };
        let negative_zero = BurnerEnergy {
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: -0.0,
            energy_usage_watts: 0.0,
        };

        assert_eq!(positive_zero, negative_zero);
        assert_eq!(hash(&positive_zero), hash(&negative_zero));
    }

    #[test]
    fn burner_energy_equality_uses_float_nan_semantics() {
        let nan_energy = BurnerEnergy {
            fuel_slot: ItemSlot::default(),
            energy_remaining_joules: f64::NAN,
            energy_usage_watts: 0.0,
        };
        let other_nan_energy = BurnerEnergy {
            fuel_slot: ItemSlot::default(),
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
