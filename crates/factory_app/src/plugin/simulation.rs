use bevy::prelude::*;
use bevy::time::{Fixed, Real, Virtual};
use factory_data::PrototypeCatalog;
use factory_sim::Simulation;

use super::AppSet;
use crate::constants::{MAX_SIM_CATCH_UP_TICKS, SIM_TICKS_PER_SECOND};
use crate::resources::{FixedStepCatchUpStats, SimProfileStats, SimResource};
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

        let fixed_time = Time::<Fixed>::from_hz(SIM_TICKS_PER_SECOND);
        let max_frame_delta = fixed_time.timestep() * MAX_SIM_CATCH_UP_TICKS;

        // Bevy's virtual-time clamp is the fixed-loop catch-up ceiling. Excess
        // wall time is dropped rather than carried into later frames, so the
        // fixed loop cannot amplify a hitch into an unbounded recovery stall.
        app.init_resource::<Time<Virtual>>();
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .set_max_delta(max_frame_delta);

        app.insert_resource(fixed_time)
            .insert_resource(sim)
            .init_resource::<SimProfileStats>()
            .init_resource::<FixedStepCatchUpStats>()
            .init_resource::<AppPauseState>()
            .init_resource::<SimCommandBacklog>()
            .add_message::<SimCommandRequest>()
            .add_message::<SimCommandResult>()
            .add_systems(
                FixedUpdate,
                (collect_sim_commands, tick_sim)
                    .chain()
                    .in_set(AppSet::SimTick),
            )
            .add_systems(
                RunFixedMainLoop,
                begin_fixed_step_frame.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            );
    }
}

/// Resets per-frame counters and records wall time rejected by the virtual-time
/// clamp before Bevy drains the resulting fixed-step work.
fn begin_fixed_step_frame(
    real_time: Res<Time<Real>>,
    virtual_time: Res<Time<Virtual>>,
    mut stats: ResMut<FixedStepCatchUpStats>,
) {
    stats.fixed_ticks_this_frame = 0;
    let dropped = real_time.delta().saturating_sub(virtual_time.max_delta());
    stats.dropped_time_this_frame = dropped;
    if !dropped.is_zero() {
        stats.capped_frames = stats.capped_frames.saturating_add(1);
        stats.total_dropped_time = stats.total_dropped_time.saturating_add(dropped);
    }
}
