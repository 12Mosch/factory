use crate::ids::EntityId;
use crate::inventory::ItemSlot;
use crate::machines::{MachineEnergy, MachineModuleState};
use crate::player::ManualMiningTarget;
use factory_data::ItemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct MiningDrillState {
    pub modules: MachineModuleState,
    pub energy: MachineEnergy,
    pub mining_progress_ticks: u32,
    pub mining_required_ticks: u32,
    pub resource_target: Option<ManualMiningTarget>,
    pub output_slot: ItemSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiningDrillError {
    MissingEntity(EntityId),
    NotMiningDrill(EntityId),
    InvalidFuel(ItemId),
    InvalidSlot {
        slot_index: usize,
    },
    EmptySlot {
        slot_index: usize,
    },
    InsufficientSpace,
    /// The machine is electric and has no fuel slot to transfer with.
    NoFuelSlot,
    UnknownItem,
}
