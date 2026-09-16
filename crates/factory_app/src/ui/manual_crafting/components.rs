use bevy::prelude::*;
use factory_data::RecipeId;
use factory_sim::{CraftingJobId, CraftingQueueMove};
use std::sync::Arc;

use crate::ui::resources::CraftingPanelTab;

#[derive(Component)]
pub(crate) struct CraftingRecipeButton {
    pub(crate) recipe_id: RecipeId,
}

#[derive(Component)]
pub(crate) struct CraftingTabButton {
    pub(crate) tab: CraftingPanelTab,
}

#[derive(Clone, Copy, Component)]
pub(crate) struct CraftingQueueButton {
    pub(crate) job_id: CraftingJobId,
    pub(crate) action: CraftingQueueAction,
}

#[derive(Clone, Copy)]
pub(crate) enum CraftingQueueAction {
    Cancel,
    Move(CraftingQueueMove),
}

#[derive(Component)]
pub(crate) struct CraftingRecipeListRoot;

#[derive(Component)]
pub(crate) struct CraftingRecipeEmpty;

#[derive(Component)]
pub(crate) struct CraftingFeedbackText;

#[derive(Component)]
pub(crate) struct CraftingRecipeRow(pub(crate) RecipeId);

#[derive(Component)]
pub(crate) struct CraftingQueueRoot;

#[derive(Component)]
pub(crate) struct CraftingQueueEmpty;

#[derive(Component)]
pub(crate) struct CraftingQueueRow(pub(crate) CraftingJobId);

#[derive(Component)]
pub(crate) struct CraftingQueueProgressFill(pub(crate) CraftingJobId);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CraftingPanelSnapshot {
    pub(crate) selected_tab: CraftingPanelTab,
    pub(crate) rows: Vec<ManualCraftRecipeRow>,
    pub(crate) queue: Vec<ManualCraftQueueRow>,
    pub(crate) feedback: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManualCraftRecipeRow {
    pub(crate) recipe_id: RecipeId,
    pub(crate) display_name: Arc<str>,
    pub(crate) products: Arc<str>,
    pub(crate) ingredients: String,
    pub(crate) status: String,
    pub(crate) button_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManualCraftQueueRow {
    pub(crate) job_id: CraftingJobId,
    pub(crate) status: String,
    pub(crate) can_move_earlier: bool,
    pub(crate) can_move_later: bool,
    pub(crate) progress_percent: u8,
}
