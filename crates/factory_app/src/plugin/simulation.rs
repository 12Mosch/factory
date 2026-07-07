use bevy::prelude::*;
use bevy::time::Fixed;
use factory_data::PrototypeCatalog;
use factory_sim::Simulation;

use super::AppSet;
use crate::constants::SIM_TICKS_PER_SECOND;
use crate::resources::{SimProfileStats, SimResource};
use crate::simulation::{SimCommandRequest, SimCommandResult, drain_sim_commands, tick_sim};

/// Owns the simulation state and runs the fixed-timestep tick.
pub(super) struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        let sim = Simulation::new(
            123,
            PrototypeCatalog::load_base().expect("base prototype catalog should load"),
        );

        app.insert_resource(Time::<Fixed>::from_hz(SIM_TICKS_PER_SECOND))
            .insert_resource(SimResource { sim })
            .init_resource::<SimProfileStats>()
            .add_message::<SimCommandRequest>()
            .add_message::<SimCommandResult>()
            .add_systems(
                FixedUpdate,
                (drain_sim_commands, tick_sim)
                    .chain()
                    .in_set(AppSet::SimTick),
            );
    }
}
