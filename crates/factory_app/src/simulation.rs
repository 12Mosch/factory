use bevy::prelude::*;
use factory_sim::{SimCommand, SimCommandEffect, SimCommandError};

use crate::resources::{SimProfileStats, SimResource, UpsStats};

/// A player command queued for the simulation. Frame-rate systems (UI clicks,
/// world input) write these; [`drain_sim_commands`] applies them at the next
/// fixed tick, so the command stream is the only way interactive input
/// mutates the simulation.
#[derive(Message)]
pub struct SimCommandRequest(pub SimCommand);

/// The outcome of an applied [`SimCommandRequest`], for frame-side feedback
/// (click sounds, transfer errors, build placement status).
#[derive(Message)]
pub struct SimCommandResult {
    pub command: SimCommand,
    pub result: Result<SimCommandEffect, SimCommandError>,
}

/// Applies all queued commands at the tick boundary, before the simulation
/// advances.
pub(crate) fn drain_sim_commands(
    mut sim: ResMut<SimResource>,
    mut requests: MessageReader<SimCommandRequest>,
    mut results: MessageWriter<SimCommandResult>,
) {
    for request in requests.read() {
        let result = sim.sim.apply_command(&request.0);
        results.write(SimCommandResult {
            command: request.0.clone(),
            result,
        });
    }
}

pub(crate) fn tick_sim(
    mut sim: ResMut<SimResource>,
    mut ups: ResMut<UpsStats>,
    mut profile_stats: ResMut<SimProfileStats>,
) {
    let profile = sim.sim.profiled_tick();
    let tick_ms = profile.total.as_secs_f64() * 1000.0;
    profile_stats.rolling_average_sim_tick_ms = if profile_stats.rolling_average_sim_tick_ms == 0.0
    {
        tick_ms
    } else {
        profile_stats.rolling_average_sim_tick_ms * 0.9 + tick_ms * 0.1
    };
    profile_stats.last_tick = profile;
    ups.fixed_ticks += 1;
}
