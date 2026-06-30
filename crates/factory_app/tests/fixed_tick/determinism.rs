use super::common::{run_until_tick, sim_tick_and_hash, test_app};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::resources::SimResource;
use factory_sim::Simulation;
use std::time::Duration;

const TARGET_TICKS: u64 = 3_600;

#[test]
fn fixed_update_hash_matches_at_60_and_144_fps() {
    let at_60_fps = super::common::run_to_tick_with_frame_rate(60.0, TARGET_TICKS);
    let at_144_fps = super::common::run_to_tick_with_frame_rate(144.0, TARGET_TICKS);

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

#[test]
fn input_movement_changes_player_position_under_fixed_ticks() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let before = app.world().resource::<SimResource>().sim.player();
    let before_tick = app.world().resource::<SimResource>().sim.tick_count();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    run_until_tick(&mut app, before_tick + 1);

    let after = app.world().resource::<SimResource>().sim.player();
    assert!(after.x_fixed() > before.x_fixed());
    assert_eq!(after.y_fixed(), before.y_fixed());
}
