use crate::entities::EntityFootprint;
use crate::ids::EntityId;
use crate::machines::BurnerEnergy;
use crate::world::WorldTileCoord;
use factory_data::ItemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricPoleState;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ElectricConsumerState {
    pub work_remainder_permyriad: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SteamEngineState;

/// Solar panels hold no durable per-entity state: their output is a pure
/// function of the shared daylight ratio and the prototype's maximum output.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SolarPanelState;

/// Durable stored energy of one accumulator.
///
/// Energy is tracked in whole joules plus a sub-joule remainder measured in
/// watt-ticks (one watt-tick is `1/60` joule at the 60 Hz simulation rate).
/// The remainder keeps charge/discharge exact without any floating-point
/// state: every tick converts watt-ticks to joules with integer division and
/// carries the leftover forward.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AccumulatorState {
    pub(crate) stored_energy_joules: u64,
    pub(crate) energy_remainder_watt_ticks: u8,
}

impl AccumulatorState {
    /// Whole joules currently stored.
    pub fn stored_energy_joules(&self) -> u64 {
        self.stored_energy_joules
    }

    /// Sub-joule remainder in watt-ticks (`0..60`), preserving exact energy
    /// between ticks.
    pub fn energy_remainder_watt_ticks(&self) -> u8 {
        self.energy_remainder_watt_ticks
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct BoilerState {
    pub energy: BurnerEnergy,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct OffshorePumpState;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PowerSummary {
    pub production_watts: u64,
    pub available_production_watts: u64,
    pub consumption_watts: u64,
    pub satisfaction_permyriad: u32,
    pub network_count: usize,
    pub accumulator_count: usize,
    pub accumulator_charge_watts: u64,
    pub accumulator_discharge_watts: u64,
    pub accumulator_stored_energy_joules: u64,
    pub accumulator_capacity_joules: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PowerNetworkSnapshot {
    pub network_id: u32,
    pub pole_count: usize,
    pub producer_count: usize,
    pub consumer_count: usize,
    pub production_watts: u64,
    pub available_production_watts: u64,
    pub consumption_watts: u64,
    pub satisfaction_permyriad: u32,
    pub accumulator_count: usize,
    pub accumulator_charge_watts: u64,
    pub accumulator_discharge_watts: u64,
    pub accumulator_stored_energy_joules: u64,
    pub accumulator_capacity_joules: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct EntityPowerStatus {
    pub network_id: Option<u32>,
    pub satisfaction_permyriad: u32,
    pub active_usage_watts: u64,
    pub drain_watts: u64,
}

/// Deterministic, region-scoped power intelligence for map presentation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerMapSnapshot {
    pub poles: Vec<PowerMapPole>,
    pub connections: Vec<PowerMapConnection>,
    pub consumers: Vec<PowerMapConsumer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerMapPole {
    pub entity_id: EntityId,
    /// Pole center in half-tile units.
    pub center_x2: WorldTileCoord,
    pub center_y2: WorldTileCoord,
    pub network_id: u32,
    pub satisfaction_permyriad: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerMapConnection {
    pub first_pole_id: EntityId,
    pub second_pole_id: EntityId,
    pub network_id: u32,
    pub satisfaction_permyriad: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerMapConsumer {
    pub entity_id: EntityId,
    pub footprint: EntityFootprint,
    pub network_id: Option<u32>,
    pub satisfaction_permyriad: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoilerError {
    MissingEntity(EntityId),
    NotBoiler(EntityId),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}
