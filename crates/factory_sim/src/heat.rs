//! Heat networks: the third transport network alongside power and fluids.
//!
//! Heat is not a fluid. A fluid flows from pressure to pressure and every unit is
//! interchangeable; heat carries a *temperature*, and what a consumer needs is not
//! a quantity of joules but a buffer hot enough to work from. That difference
//! drives the model here:
//!
//! * Durable state is energy, not temperature ([`HeatBufferState`]). Energy is
//!   what conserves exactly under integer arithmetic; temperature is derived.
//! * Energy is measured *above* [`factory_data::HEAT_AMBIENT_TEMPERATURE_DEGREES`],
//!   so a cold network holds exactly zero and can never be drained below ambient.
//! * A connected network settles to one temperature each tick. Equal temperature
//!   means each buffer holds energy in proportion to its specific heat, with any
//!   buffer that would exceed its own maximum temperature clamped and its surplus
//!   redistributed to the rest.
//!
//! The consequence players feel is a warm-up: a fresh reactor has to raise the
//! whole network's thermal mass past each exchanger's minimum working temperature
//! before any steam is produced, and a network that loses its reactor keeps
//! delivering until its stored heat runs out.
//!
//! Connectivity and cache invalidation follow the fluid-network shape (a cached
//! topology rebuilt on placement changes, per-network dirty flags for the solve
//! and the snapshots); the solve itself is the part that differs.

use crate::ids::EntityId;
use crate::inventory::ItemSlot;
use crate::machines::BurnerEnergy;
use serde::{Deserialize, Serialize};

/// Durable heat content of one buffer, in joules above the ambient temperature.
///
/// Temperature is deliberately not stored: deriving it from energy keeps the
/// network solve exactly energy-conserving and makes save state independent of
/// the display precision.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatBufferState {
    pub energy_joules: u64,
}

/// A reactor's fuel cell and the spent cell it leaves behind.
///
/// Fuel accounting reuses [`BurnerEnergy`], so a reactor consumes fuel by the
/// same rules a furnace does; what differs is where the energy goes (its own heat
/// buffer, not work) and that the burnt fuel leaves a residue behind.
#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct NuclearReactorState {
    pub energy: BurnerEnergy,
    /// Spent fuel cells waiting to be taken away for reprocessing. A reactor
    /// refuses to start a cell whose residue would not fit here.
    pub output_slot: ItemSlot,
}

/// Heat pipes hold no state beyond their buffer: they exist to connect
/// neighbours, and their thermal mass comes from the prototype.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatPipeState;

/// Heat exchangers hold no state beyond their buffer and fluid boxes; how much
/// steam they make is a pure function of buffer temperature and prototype rates.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatExchangerState;

/// Presentation view of one settled heat network.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatNetworkSnapshot {
    pub network_id: u32,
    pub buffer_count: usize,
    pub energy_joules: u64,
    pub capacity_joules: u64,
    /// Settled network temperature in millidegrees. Millidegrees keep a readout
    /// useful at the sub-degree changes a single tick produces.
    pub temperature_millidegrees: u64,
    pub buffers: Vec<HeatNetworkBufferSnapshot>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HeatNetworkBufferSnapshot {
    pub entity_id: EntityId,
    pub energy_joules: u64,
    pub capacity_joules: u64,
    pub temperature_millidegrees: u64,
}

/// Heat status of one entity, for machine panels and diagnostics.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityHeatStatus {
    pub network_id: Option<u32>,
    pub energy_joules: u64,
    pub capacity_joules: u64,
    pub temperature_millidegrees: u64,
}

/// Temperature of a buffer holding `energy_joules`, in millidegrees.
///
/// Uses 128-bit intermediates so a large specific heat cannot overflow the
/// millidegree scaling, and reports ambient for a buffer with no thermal mass.
pub fn temperature_millidegrees(energy_joules: u64, specific_heat_joules_per_degree: u64) -> u64 {
    let ambient_millidegrees =
        u64::from(factory_data::HEAT_AMBIENT_TEMPERATURE_DEGREES).saturating_mul(1_000);
    if specific_heat_joules_per_degree == 0 {
        return ambient_millidegrees;
    }

    let above_ambient =
        (u128::from(energy_joules) * 1_000) / u128::from(specific_heat_joules_per_degree);
    ambient_millidegrees.saturating_add(above_ambient.min(u128::from(u64::MAX)) as u64)
}

/// Energy a buffer must hold to reach `temperature_degrees`.
///
/// Used to gate heat consumers on their minimum working temperature without ever
/// converting stored energy to a lossy temperature first.
pub fn energy_for_temperature(
    temperature_degrees: u32,
    specific_heat_joules_per_degree: u64,
) -> u64 {
    let degrees_above_ambient =
        temperature_degrees.saturating_sub(factory_data::HEAT_AMBIENT_TEMPERATURE_DEGREES);
    specific_heat_joules_per_degree.saturating_mul(u64::from(degrees_above_ambient))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NuclearReactorError {
    MissingEntity(EntityId),
    NotNuclearReactor(EntityId),
    InvalidFuel(crate::prototypes::ItemId),
    /// An item in the spent-fuel output slot was refused. Kept separate from
    /// [`Self::InvalidFuel`] so a rejection always names the slot it came from
    /// rather than describing a spent cell as invalid fuel.
    InvalidOutput(crate::prototypes::ItemId),
    InvalidSlot {
        slot_index: usize,
    },
    EmptySlot {
        slot_index: usize,
    },
    InsufficientSpace,
    UnknownItem,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_energy_reports_ambient_temperature() {
        assert_eq!(
            temperature_millidegrees(0, 1_000_000),
            u64::from(factory_data::HEAT_AMBIENT_TEMPERATURE_DEGREES) * 1_000
        );
    }

    #[test]
    fn temperature_and_energy_conversions_agree() {
        let specific_heat = 100_000;
        let energy = energy_for_temperature(500, specific_heat);
        assert_eq!(energy, 100_000 * (500 - 15));
        assert_eq!(temperature_millidegrees(energy, specific_heat), 500_000);
    }

    /// Sub-degree resolution is the point of the millidegree scale: one tick of a
    /// reactor moves a network by a fraction of a degree.
    #[test]
    fn temperature_resolves_below_one_degree() {
        assert_eq!(temperature_millidegrees(50_000, 100_000), 15_500);
    }

    #[test]
    fn zero_thermal_mass_reports_ambient_instead_of_dividing_by_zero() {
        assert_eq!(temperature_millidegrees(1_000, 0), 15_000);
    }
}
