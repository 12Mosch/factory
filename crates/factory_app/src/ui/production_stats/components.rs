use bevy::prelude::*;

use crate::ui::resources::StatsTab;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductionStatsSnapshot {
    pub(super) selected_tab: StatsTab,
    pub(super) item_rows: Vec<ItemStatDisplayRow>,
    pub(super) fluid_rows: Vec<ItemStatDisplayRow>,
    pub(super) power_lines: Vec<String>,
    pub(super) power_graph: Vec<PowerGraphPoint>,
    pub(super) diagnostic_lines: Vec<String>,
    pub(super) bottleneck_lines: Vec<String>,
}

#[derive(Component)]
pub struct ProductionStatsTabButton {
    pub(super) tab: StatsTab,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemStatDisplayRow {
    pub item_name: String,
    pub per_minute: String,
    pub total: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerGraphPoint {
    pub production_watts: u64,
    pub consumption_watts: u64,
}
