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

/// FIFO commands collected between fixed simulation ticks.
#[derive(Resource, Default)]
pub struct SimCommandBacklog(pub Vec<SimCommand>);

/// Applies all queued commands at the tick boundary, before the simulation
/// advances.
pub(crate) fn collect_sim_commands(
    mut requests: MessageReader<SimCommandRequest>,
    mut backlog: ResMut<SimCommandBacklog>,
) {
    for request in requests.read() {
        backlog.0.push(request.0.clone());
    }
}

/// Applies retained commands in FIFO order, then completes one simulation tick.
pub(crate) fn tick_sim(
    mut sim: ResMut<SimResource>,
    mut backlog: ResMut<SimCommandBacklog>,
    mut results: MessageWriter<SimCommandResult>,
    mut ups: ResMut<UpsStats>,
    mut profile_stats: ResMut<SimProfileStats>,
) {
    let mut simulation = sim.write();

    for command in backlog.0.drain(..) {
        let result = simulation.apply_command(&command);
        results.write(SimCommandResult { command, result });
    }
    let profile = simulation.profiled_tick();
    drop(simulation);
    sim.set_changed();
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

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::Simulation;

    use crate::resources::{SimProfileStats, UpsStats};

    #[test]
    /// Verifies that a fixed tick drains each queued command exactly once.
    fn fixed_tick_applies_collected_commands_exactly_once() {
        let mut app = App::new();
        app.insert_resource(SimResource::new(Simulation::new_test_world(123)))
            .init_resource::<SimCommandBacklog>()
            .init_resource::<SimProfileStats>()
            .init_resource::<UpsStats>()
            .add_message::<SimCommandRequest>()
            .add_message::<SimCommandResult>()
            .add_systems(Update, (collect_sim_commands, tick_sim).chain());

        let before_tick = app.world().resource::<SimResource>().read().tick_count();
        let before_position = app
            .world()
            .resource::<SimResource>()
            .read()
            .player()
            .position_tiles();
        app.world_mut()
            .write_message(SimCommandRequest(SimCommand::MovePlayer {
                direction_x: 1.0,
                direction_y: 0.0,
                delta_seconds: 1.0,
            }));

        app.update();
        assert_eq!(
            app.world().resource::<SimResource>().read().tick_count(),
            before_tick + 1
        );
        assert!(app.world().resource::<SimCommandBacklog>().0.is_empty());
        let after_command_position = app
            .world()
            .resource::<SimResource>()
            .read()
            .player()
            .position_tiles();
        assert_ne!(after_command_position, before_position);

        app.update();
        assert_eq!(
            app.world().resource::<SimResource>().read().tick_count(),
            before_tick + 2
        );
        assert_eq!(
            app.world()
                .resource::<SimResource>()
                .read()
                .player()
                .position_tiles(),
            after_command_position
        );
    }
}
