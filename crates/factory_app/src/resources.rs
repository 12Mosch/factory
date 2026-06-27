use bevy::prelude::Resource;
use factory_sim::{Direction, EntityId, Simulation};

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
pub struct DebugInventorySelection {
    pub selected_index: usize,
}

#[derive(Resource, Default)]
pub struct OpenContainer {
    pub entity_id: Option<EntityId>,
}

#[derive(Resource, Default)]
pub struct DebugBuildDirection {
    pub direction: Direction,
}
