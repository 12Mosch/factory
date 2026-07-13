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
