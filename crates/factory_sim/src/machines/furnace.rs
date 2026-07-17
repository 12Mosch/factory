use crate::ids::EntityId;
use crate::inventory::ItemSlot;
use crate::machines::MachineEnergy;
use factory_data::{ItemId, RecipeId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Hash, Serialize)]
pub struct FurnaceState {
    pub input_slot: ItemSlot,
    pub energy: MachineEnergy,
    pub output_slot: ItemSlot,
    pub active_recipe: Option<RecipeId>,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceError {
    MissingEntity(EntityId),
    NotFurnace(EntityId),
    InvalidInput(ItemId),
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
