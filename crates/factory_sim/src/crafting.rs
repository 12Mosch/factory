use factory_data::RecipeId;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CraftingQueue {
    pub entries: VecDeque<CraftingJob>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CraftingJob {
    pub recipe_id: RecipeId,
    pub remaining_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftingError {
    MissingRecipe(RecipeId),
    NotManualRecipe(RecipeId),
    RecipeLocked(RecipeId),
    InsufficientIngredients,
}
