use factory_data::RecipeId;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CraftingQueue {
    pub entries: VecDeque<CraftingJob>,
    pub next_job_id: u64,
    pub completed_jobs: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CraftingJob {
    /// Stable within one simulation, including across save/load. Ingredients
    /// are reserved from the immutable recipe catalog when this job is added.
    pub id: CraftingJobId,
    pub recipe_id: RecipeId,
    pub remaining_ticks: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CraftingJobId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum CraftingQueueMove {
    Earlier,
    Later,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftingError {
    MissingRecipe(RecipeId),
    NotManualRecipe(RecipeId),
    RecipeLocked(RecipeId),
    InsufficientIngredients,
    RefundInventoryFull,
    MissingJob(CraftingJobId),
    JobIdExhausted,
}
