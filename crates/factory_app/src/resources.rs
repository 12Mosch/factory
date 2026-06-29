use bevy::prelude::{Handle, Image, Resource};
use factory_data::{EntityPrototypeId, ItemId, TechnologyId};
use factory_sim::{Direction, EntityId, Simulation, SimulationTickProfile};
use std::time::Duration;

#[derive(Resource)]
pub struct SimResource {
    pub sim: Simulation,
}

#[derive(Resource, Default)]
pub(crate) struct UpsStats {
    pub(crate) elapsed: f64,
    pub(crate) fixed_ticks: u32,
    pub ups: f64,
}

#[derive(Resource, Default)]
pub struct SimProfileStats {
    pub last_tick: SimulationTickProfile,
    pub rolling_average_sim_tick_ms: f64,
}

#[derive(Resource, Default)]
pub struct RenderSyncStats {
    pub player: Duration,
    pub resources: Duration,
    pub placed_entities: Duration,
    pub belt_directions: Duration,
    pub belt_items: Duration,
    pub total: Duration,
}

impl RenderSyncStats {
    pub fn record_player(&mut self, elapsed: Duration) {
        self.player = elapsed;
        self.update_total();
    }

    pub fn record_resources(&mut self, elapsed: Duration) {
        self.resources = elapsed;
        self.update_total();
    }

    pub fn record_placed_entities(&mut self, elapsed: Duration) {
        self.placed_entities = elapsed;
        self.update_total();
    }

    pub fn record_belt_directions(&mut self, elapsed: Duration) {
        self.belt_directions = elapsed;
        self.update_total();
    }

    pub fn record_belt_items(&mut self, elapsed: Duration) {
        self.belt_items = elapsed;
        self.update_total();
    }

    fn update_total(&mut self) {
        self.total = self.player
            + self.resources
            + self.placed_entities
            + self.belt_directions
            + self.belt_items;
    }
}

#[derive(Resource, Default)]
pub struct OpenContainer {
    pub entity_id: Option<EntityId>,
}

#[derive(Resource, Default)]
pub struct TechnologyWindowState {
    pub open: bool,
    pub selected: Option<TechnologyId>,
}

#[derive(Resource, Default)]
pub struct MapViewState {
    pub open: bool,
}

#[derive(Resource, Default)]
pub struct MapDisplaySettings {
    pub debug_reveal_all: bool,
    pub show_chunk_grid: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapTextureBounds {
    pub min_x: i32,
    pub min_y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Resource, Default)]
pub struct MapTextureCache {
    pub handle: Option<Handle<Image>>,
    pub bounds: Option<MapTextureBounds>,
    pub last_player_tile: Option<(i32, i32)>,
    pub last_resource_revision: u64,
    pub last_entity_signature: u64,
    pub last_revealed_signature: u64,
    pub last_debug_flags: (bool, bool),
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
}

#[derive(Resource, Default)]
pub struct AppInputState {
    pub world_blocked: bool,
    pub escape_consumed: bool,
}

#[derive(Resource, Default)]
pub struct BuildPlacementState {
    pub selected: Option<BuildSelection>,
    pub direction: Direction,
    pub last_status: BuildPlacementStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildSelection {
    pub prototype_id: EntityPrototypeId,
    pub item_id: ItemId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BuildPlacementStatus {
    #[default]
    Ready,
    Placed(String),
    CannotPlace(String),
    MissingInventory(String),
    Locked(String),
}
