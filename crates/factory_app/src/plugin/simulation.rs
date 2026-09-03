use bevy::prelude::*;
use bevy::time::Fixed;
use factory_data::PrototypeCatalog;
use factory_sim::Simulation;

use super::AppSet;
use crate::constants::SIM_TICKS_PER_SECOND;
use crate::resources::{SimProfileStats, SimResource};
use crate::simulation::{
    AppPauseState, SimCommandBacklog, SimCommandRequest, SimCommandResult, collect_sim_commands,
    tick_sim,
};
use crate::world_setup::StartInWorldSetup;

/// Owns the simulation state and runs the fixed-timestep tick.
pub(super) struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    /// Installs either the explicit pre-game state or the legacy direct-start world.
    fn build(&self, app: &mut App) {
        let sim = if app.world().contains_resource::<StartInWorldSetup>() {
            SimResource::empty()
        } else {
            SimResource::new(
                Simulation::new(
                    123,
                    PrototypeCatalog::load_base().expect("base prototype catalog should load"),
                )
                .expect("base prototype catalog should construct a simulation"),
            )
        };

        app.insert_resource(Time::<Fixed>::from_hz(SIM_TICKS_PER_SECOND))
            .insert_resource(sim)
            .init_resource::<SimProfileStats>()
            .init_resource::<AppPauseState>()
            .init_resource::<SimCommandBacklog>()
            .add_message::<SimCommandRequest>()
            .add_message::<SimCommandResult>()
            .add_systems(
                FixedUpdate,
                (collect_sim_commands, tick_sim)
                    .chain()
                    .in_set(AppSet::SimTick),
            );
    }
}
