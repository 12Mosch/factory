use bevy::prelude::Resource;
use factory_data::TechnologyId;
use factory_sim::EntityId;

#[derive(Resource, Default)]
pub struct OpenContainer {
    pub entity_id: Option<EntityId>,
}

#[derive(Resource, Default)]
pub struct InventoryTransferFeedback {
    pub message: Option<String>,
}

#[derive(Resource, Default)]
pub struct TechnologyWindowState {
    pub open: bool,
    pub selected: Option<TechnologyId>,
}

#[derive(Resource)]
pub struct CraftingWindowState {
    pub open: bool,
    pub selected_tab: CraftingPanelTab,
}

impl Default for CraftingWindowState {
    fn default() -> Self {
        Self {
            open: false,
            selected_tab: CraftingPanelTab::Player,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CraftingPanelTab {
    Player,
    Smelting,
    Assembling,
}

#[derive(Resource)]
pub struct ProductionStatsWindowState {
    pub open: bool,
    pub selected_tab: StatsTab,
}

impl Default for ProductionStatsWindowState {
    fn default() -> Self {
        Self {
            open: false,
            selected_tab: StatsTab::Production,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatsTab {
    Production,
    Consumption,
    Power,
    Diagnostics,
}
