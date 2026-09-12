use bevy::prelude::*;
use factory_data::{FluidId, ItemId};

use crate::ui::resources::StatsTab;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProductionStatsSnapshot {
    pub(super) selected_tab: StatsTab,
    pub(super) rockets_launched: u64,
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

#[derive(Component)]
pub(super) struct ProductionStatsRocketsText;

#[derive(Component)]
pub(super) struct ProductionStatsBody;

#[derive(Clone, Copy, Component, Debug, PartialEq, Eq)]
pub(super) enum StatSection {
    Items,
    Fluids,
}

#[derive(Component)]
pub(super) struct StatRowsRoot(pub(super) StatSection);

#[derive(Component)]
pub(super) struct StatEmptyLabel(pub(super) StatSection);

#[derive(Component, Debug, PartialEq, Eq)]
pub(super) struct StatRow {
    pub(super) section: StatSection,
    pub(super) key: StatRowKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StatRowKey {
    Item(ItemId),
    Fluid(FluidId),
}

#[derive(Component)]
pub(super) struct PowerLinesRoot;

#[derive(Component)]
pub(super) struct PowerLine(pub(super) usize);

#[derive(Component)]
pub(super) struct PowerGraphRoot;

#[derive(Component)]
pub(super) struct PowerGraphEmpty;

#[derive(Clone, Copy, Component, Debug, PartialEq, Eq)]
pub(super) struct PowerGraphBar {
    pub(super) index: usize,
    pub(super) production: bool,
}

#[derive(Clone, Copy, Component, Debug, PartialEq, Eq)]
pub(super) enum DiagnosticSection {
    Status,
    Bottleneck,
}

#[derive(Component)]
pub(super) struct DiagnosticLinesRoot(pub(super) DiagnosticSection);

#[derive(Component)]
pub(super) struct DiagnosticLine {
    pub(super) section: DiagnosticSection,
    pub(super) index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemStatDisplayRow {
    pub(crate) key: StatRowKey,
    pub item_name: String,
    pub per_minute: String,
    pub total: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerGraphPoint {
    pub production_watts: u64,
    pub consumption_watts: u64,
}
