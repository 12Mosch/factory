use bevy::prelude::*;

use crate::resources::{SimProfileStats, SimResource, UpsStats};

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
