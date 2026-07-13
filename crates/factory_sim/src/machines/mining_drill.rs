use crate::ids::EntityId;
use crate::inventory::ItemSlot;
use crate::machines::BurnerEnergy;
use crate::player::ManualMiningTarget;
use factory_data::ItemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct BurnerMiningDrillState {
    pub energy: BurnerEnergy,
    pub mining_progress_ticks: u32,
    pub mining_required_ticks: u32,
    pub resource_target: Option<ManualMiningTarget>,
    pub output_slot: ItemSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnerDrillError {
    MissingEntity(EntityId),
    NotBurnerDrill(EntityId),
    InvalidFuel(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}
