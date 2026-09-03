use bevy::prelude::*;
use factory_sim::{SimCommand, SimCommandEffect, SimCommandError};

use crate::input::resources::{TrainManualInput, WeaponInput};
use crate::resources::{FixedStepCatchUpStats, SimProfileStats, SimResource, UpsStats};

/// Presentation-owned pause state for the active game session.
///
/// This deliberately lives outside [`factory_sim::Simulation`]: pausing is an
/// application concern, is not serialized, and must not participate in the
/// deterministic simulation hash.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppPauseState {
    paused: bool,
}

impl AppPauseState {
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }
}

/// Run condition for fixed-step systems that may advance simulation state.
pub(crate) fn simulation_running(pause: Res<AppPauseState>) -> bool {
    !pause.is_paused()
}

/// Run condition for frame-side command buffering while simulation is paused.
pub(crate) fn simulation_paused(pause: Res<AppPauseState>) -> bool {
    pause.is_paused()
}

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

/// Drops transient input that was waiting when pause began.
///
/// Commands deliberately issued from responsive paused UI (for example enemy
/// settings) are written after this transition and continue to collect in FIFO
/// order. Only input aimed at the world before the pause boundary is discarded.
pub(crate) fn discard_transient_input_on_pause(
    pause: Res<AppPauseState>,
    mut requests: ResMut<Messages<SimCommandRequest>>,
    mut backlog: ResMut<SimCommandBacklog>,
    mut train_input: ResMut<TrainManualInput>,
    mut weapon_input: ResMut<WeaponInput>,
) {
    if !pause.is_changed() || !pause.is_paused() {
        return;
    }

    requests.clear();
    backlog.0.clear();
    train_input.clear();
    weapon_input.clear();
}

/// Applies all queued commands at the tick boundary, before the simulation
/// advances.
pub(crate) fn collect_sim_commands(
    mut requests: ResMut<Messages<SimCommandRequest>>,
    mut backlog: ResMut<SimCommandBacklog>,
) {
    for request in requests.drain() {
        backlog.0.push(request.0);
    }
}

/// Applies retained commands in FIFO order, then completes one simulation tick.
pub(crate) fn tick_sim(
    mut sim: ResMut<SimResource>,
    mut backlog: ResMut<SimCommandBacklog>,
    mut results: MessageWriter<SimCommandResult>,
    mut ups: ResMut<UpsStats>,
    mut profile_stats: ResMut<SimProfileStats>,
    mut catch_up_stats: ResMut<FixedStepCatchUpStats>,
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
    catch_up_stats.fixed_ticks_this_frame += 1;
    catch_up_stats.peak_fixed_ticks_per_frame = catch_up_stats
        .peak_fixed_ticks_per_frame
        .max(catch_up_stats.fixed_ticks_this_frame);
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{EnemyDifficultyPreset, Simulation, load_from_bytes, save_to_bytes};

    use crate::input::resources::TrainManualInput;
    use crate::resources::{SimProfileStats, UpsStats};

    fn pause_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(SimResource::new(Simulation::new_test_world(123)))
            .init_resource::<AppPauseState>()
            .init_resource::<SimCommandBacklog>()
            .init_resource::<TrainManualInput>()
            .init_resource::<WeaponInput>()
            .init_resource::<SimProfileStats>()
            .init_resource::<FixedStepCatchUpStats>()
            .init_resource::<UpsStats>()
            .add_message::<SimCommandRequest>()
            .add_message::<SimCommandResult>()
            .add_systems(
                PreUpdate,
                (
                    discard_transient_input_on_pause,
                    collect_sim_commands.run_if(simulation_paused),
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (collect_sim_commands, tick_sim)
                    .chain()
                    .run_if(simulation_running),
            );
        app
    }

    #[test]
    /// Verifies that a fixed tick drains each queued command exactly once.
    fn fixed_tick_applies_collected_commands_exactly_once() {
        let mut app = App::new();
        app.insert_resource(SimResource::new(Simulation::new_test_world(123)))
            .init_resource::<SimCommandBacklog>()
            .init_resource::<SimProfileStats>()
            .init_resource::<FixedStepCatchUpStats>()
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

    #[test]
    fn paused_fixed_steps_leave_tick_and_deterministic_hash_unchanged() {
        let mut app = pause_test_app();
        app.world_mut().run_schedule(FixedUpdate);

        app.world_mut().resource_mut::<AppPauseState>().pause();
        app.world_mut().run_schedule(PreUpdate);
        let before = {
            let sim = app.world().resource::<SimResource>().read();
            (sim.tick_count(), sim.state_hash())
        };

        for _ in 0..128 {
            app.world_mut().run_schedule(FixedUpdate);
        }

        let after = {
            let sim = app.world().resource::<SimResource>().read();
            (sim.tick_count(), sim.state_hash())
        };
        assert_eq!(after, before);
    }

    #[test]
    fn paused_commands_resume_once_in_fifo_order_without_stale_input() {
        let mut app = pause_test_app();
        let stale = EnemyDifficultyPreset::Peaceful.config().runtime;
        let first = EnemyDifficultyPreset::Aggressive.config().runtime;
        let second = EnemyDifficultyPreset::Standard.config().runtime;

        app.world_mut()
            .write_message(SimCommandRequest(SimCommand::SetEnemyRuntimeSettings(
                stale,
            )));
        app.world_mut().resource_mut::<AppPauseState>().pause();
        app.world_mut().run_schedule(PreUpdate);

        app.world_mut()
            .write_message(SimCommandRequest(SimCommand::SetEnemyRuntimeSettings(
                first,
            )));
        app.world_mut().run_schedule(PreUpdate);
        app.world_mut().run_schedule(FixedUpdate);
        app.world_mut()
            .write_message(SimCommandRequest(SimCommand::SetEnemyRuntimeSettings(
                second,
            )));
        app.world_mut().run_schedule(PreUpdate);
        app.world_mut().run_schedule(FixedUpdate);

        let paused_tick = app.world().resource::<SimResource>().read().tick_count();
        assert_eq!(app.world().resource::<SimCommandBacklog>().0.len(), 2);
        assert_ne!(
            app.world()
                .resource::<SimResource>()
                .read()
                .enemy_settings()
                .runtime,
            stale,
            "the command waiting before pause must be discarded"
        );

        app.world_mut().resource_mut::<AppPauseState>().resume();
        app.world_mut().run_schedule(FixedUpdate);

        let sim = app.world().resource::<SimResource>().read();
        assert_eq!(sim.tick_count(), paused_tick + 1);
        assert_eq!(sim.enemy_settings().runtime, second);
        assert!(app.world().resource::<SimCommandBacklog>().0.is_empty());
    }

    #[test]
    fn save_load_round_trip_does_not_serialize_or_clear_app_pause() {
        let mut app = pause_test_app();
        app.world_mut().resource_mut::<AppPauseState>().pause();
        app.world_mut().run_schedule(PreUpdate);

        let (bytes, expected) = {
            let sim = app.world().resource::<SimResource>().read();
            (
                save_to_bytes(&sim).expect("paused simulation should save"),
                (sim.tick_count(), sim.state_hash()),
            )
        };
        let loaded = load_from_bytes(&bytes).expect("paused save should load");
        app.world_mut()
            .resource_mut::<SimResource>()
            .replace(loaded)
            .expect("test simulation should not be locked");

        let actual = {
            let sim = app.world().resource::<SimResource>().read();
            (sim.tick_count(), sim.state_hash())
        };
        assert_eq!(actual, expected);
        assert!(app.world().resource::<AppPauseState>().is_paused());
    }
}
