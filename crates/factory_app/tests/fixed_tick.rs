use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::{FactoryAppPlugin, SimResource};
use factory_sim::Simulation;
use std::time::Duration;

const TARGET_TICKS: u64 = 3_600;

#[test]
fn fixed_update_hash_matches_at_60_and_144_fps() {
    let at_60_fps = run_to_tick_with_frame_rate(60.0, TARGET_TICKS);
    let at_144_fps = run_to_tick_with_frame_rate(144.0, TARGET_TICKS);

    assert_eq!(at_60_fps.0, TARGET_TICKS);
    assert_eq!(at_144_fps.0, TARGET_TICKS);
    assert_eq!(at_60_fps.1, at_144_fps.1);
}

#[test]
fn zero_duration_render_pause_does_not_advance_or_corrupt_sim() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    run_until_tick(&mut app, 120);

    let before_pause = sim_tick_and_hash(&app);
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));

    for _ in 0..240 {
        app.update();
    }

    assert_eq!(sim_tick_and_hash(&app), before_pause);

    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    run_until_tick(&mut app, TARGET_TICKS);

    let mut expected = Simulation::new_test_world(123);
    for _ in 0..TARGET_TICKS {
        expected.tick();
    }

    assert_eq!(
        sim_tick_and_hash(&app),
        (TARGET_TICKS, expected.state_hash())
    );
}

fn run_to_tick_with_frame_rate(frame_rate: f64, target_tick: u64) -> (u64, u64) {
    let mut app = test_app(Duration::from_secs_f64(1.0 / frame_rate));
    run_until_tick(&mut app, target_tick);
    sim_tick_and_hash(&app)
}

fn test_app(frame_duration: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration));
    app
}

fn run_until_tick(app: &mut App, target_tick: u64) {
    while app.world().resource::<SimResource>().sim.tick_count() < target_tick {
        app.update();
    }
}

fn sim_tick_and_hash(app: &App) -> (u64, u64) {
    let sim = &app.world().resource::<SimResource>().sim;
    (sim.tick_count(), sim.state_hash())
}
