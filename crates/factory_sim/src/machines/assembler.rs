use crate::ids::EntityId;
use crate::inventory::Inventory;
use factory_data::{ItemId, RecipeId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AssemblingMachineState {
    pub selected_recipe: Option<RecipeId>,
    pub input_inventory: Inventory,
    pub output_inventory: Inventory,
    pub crafting_progress_ticks: u32,
    pub crafting_required_ticks: u32,
    pub crafting_speed_numerator: u32,
    pub crafting_speed_denominator: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssemblerIngredientStatus {
    pub item: ItemId,
    pub required: u32,
    pub available: u32,
    pub missing: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssemblerError {
    MissingEntity(EntityId),
    NotAssembler(EntityId),
    MissingRecipe(RecipeId),
    InvalidRecipe(RecipeId),
    RecipeLocked(RecipeId),
    RecipeChangeRequiresEmpty { entity_id: EntityId },
    InvalidInput(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}
