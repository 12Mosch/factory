use bevy::prelude::Resource;
use factory_data::TechnologyId;
use factory_sim::EntityId;
use factory_sim::PlayerEquipmentError;
use factory_sim::RollingStockId;

/// What the player has open, if anything.
///
/// At most one of the two at a time — opening either closes the other — so the
/// two windows never fight over the shared slot grid, and a system that only
/// understands entities sees `None` while a wagon is open rather than something
/// it would misread.
#[derive(Resource, Default)]
pub struct OpenContainer {
    pub entity_id: Option<EntityId>,
    pub rolling_stock: Option<RollingStockId>,
}

impl OpenContainer {
    pub fn close(&mut self) {
        self.entity_id = None;
        self.rolling_stock = None;
    }

    pub fn is_open(&self) -> bool {
        self.entity_id.is_some() || self.rolling_stock.is_some()
    }
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

#[derive(Resource, Default)]
pub struct EquipmentWindowState {
    pub open: bool,
    pub selected_inventory_slot: Option<usize>,
    pub feedback: Option<String>,
    pub last_error: Option<PlayerEquipmentError>,
}

#[derive(Resource)]
pub struct CraftingWindowState {
    pub open: bool,
    pub selected_tab: CraftingPanelTab,
    pub feedback: Option<String>,
}

impl Default for CraftingWindowState {
    fn default() -> Self {
        Self {
            open: false,
            selected_tab: CraftingPanelTab::Player,
            feedback: None,
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
