use bevy::prelude::*;
use factory_data::RecipeId;

use crate::resources::CraftingPanelTab;

#[derive(Component)]
pub(crate) struct CraftingPanelRoot {
    pub(crate) snapshot: CraftingPanelSnapshot,
}

#[derive(Component)]
pub(crate) struct CraftingRecipeButton {
    pub(crate) recipe_id: RecipeId,
}

#[derive(Component)]
pub(crate) struct CraftingTabButton {
    pub(crate) tab: CraftingPanelTab,
}

#[derive(Component)]
pub(crate) struct CraftingQueueRoot {
    pub(crate) lines: Vec<String>,
}

#[derive(Component)]
pub(crate) struct CraftingRecipeListRoot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CraftingPanelSnapshot {
    pub(crate) selected_tab: CraftingPanelTab,
    pub(crate) rows: Vec<ManualCraftRecipeRow>,
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
