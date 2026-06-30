use crate::ids::EntityId;
use crate::machines::BurnerEnergy;
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
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
