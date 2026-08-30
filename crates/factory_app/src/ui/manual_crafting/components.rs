use bevy::prelude::*;
use factory_data::RecipeId;
use factory_sim::{CraftingJobId, CraftingQueueMove};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CraftingQueueSnapshot(pub(crate) Vec<ManualCraftQueueRow>);

#[derive(Component)]
pub(crate) struct CraftingRecipeListRoot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CraftingPanelSnapshot {
    pub(crate) selected_tab: CraftingPanelTab,
    pub(crate) rows: Vec<ManualCraftRecipeRow>,
    pub(crate) feedback: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManualCraftRecipeRow {
    pub(crate) recipe_id: RecipeId,
    pub(crate) display_name: String,
    pub(crate) products: String,
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
}
