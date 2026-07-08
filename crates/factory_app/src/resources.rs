use bevy::prelude::Resource;
use factory_sim::{Simulation, SimulationTickProfile};

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
