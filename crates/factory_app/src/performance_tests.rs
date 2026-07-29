use bevy::audio::{AudioPlayer, AudioSource};
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_sim::Simulation;
use std::time::Duration;

use crate::FactoryAppPlugin;
use crate::audio::{
    AudioAssets, AudioEventDedupe, AudioSettings, AudioSettingsWindowState, MachineAudioLoops,
    SoundEvent, play_sound_events, sync_machine_audio_loops,
};
use crate::rendering::resources::VisibleEntityIds;
use crate::rendering::robots::RobotSprite;
use crate::rendering::rolling_stock::RollingStockSprite;
use crate::resources::SimResource;
use crate::test_performance::{
    BENCHMARK_LOCK, PerformanceBudget, assert_performance_budget, collect_performance_stats,
    collect_prepared_performance_stats, measure_performance_sample, print_performance_stats,
};
use crate::ui::audio_settings::sync_audio_settings_window;
use crate::ui::enemy_settings::{EnemySettingsWindowState, sync_enemy_settings_window};
use crate::ui::manual_crafting::sync_manual_crafting_panel;
use crate::ui::objectives_panel::sync_objectives_panel;
use crate::ui::production_stats::sync_production_stats_window;
use crate::ui::resources::{
    CraftingWindowState, ProductionStatsWindowState, TechnologyWindowState,
};
use crate::ui::technology_panel::sync_technology_panel;
use crate::ui::threat::sync_threat_ui;

const WARMUP_FRAMES: usize = 30;
/// Robots dispatched for the robot-heavy frame benchmark. Well past what a
/// single roboport network holds, so the measured frame covers robots flying,
/// arriving, queueing for pads, charging, and docking at the same time.
const BENCHMARK_ROBOTS: usize = 4_000;
/// Trains driven for the train-heavy frame benchmark. Far more than a real
/// factory runs — the point is a per-train step that stays cheap when the
/// railway grows, not a plausible base — and each is a locomotive with two
/// wagons, so the measured frame covers three times as many bodies walking the
/// rail graph and being drawn along it.
const BENCHMARK_TRAINS: usize = 64;
const MEASUREMENT_FRAMES: usize = 300;
const AUDIO_EVENTS_PER_FRAME: usize = 128;

const AUDIO_BUDGET: PerformanceBudget = PerformanceBudget {
    p99: Duration::from_millis(2),
    hitch: Duration::from_millis(8),
    alloc_p99_bytes: 128 * 1024,
    alloc_hitch_bytes: 512 * 1024,
    alloc_p99_count: 512,
    alloc_hitch_count: 2_048,
};
const UI_BUDGET: PerformanceBudget = PerformanceBudget {
    p99: Duration::from_millis(4),
    hitch: Duration::from_millis(8),
    alloc_p99_bytes: 256 * 1024,
    alloc_hitch_bytes: 1024 * 1024,
    alloc_p99_count: 2_048,
    alloc_hitch_count: 8_192,
};
const FULL_FRAME_BUDGET: PerformanceBudget = PerformanceBudget {
    p99: Duration::from_nanos(16_667_000),
    hitch: Duration::from_nanos(33_334_000),
    alloc_p99_bytes: 2 * 1024 * 1024,
    alloc_hitch_bytes: 8 * 1024 * 1024,
    alloc_p99_count: 4_096,
    alloc_hitch_count: 16_384,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, ScheduleLabel)]
struct UiBenchmark;

/// Exercises the two per-frame audio hot paths without requiring an audio
/// device: visible-machine selection and a burst of queued one-shot sounds.
#[test]
#[ignore]
fn audio_frame_p99_hitch_and_allocation_budget() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut app = audio_benchmark_app();

    for _ in 0..WARMUP_FRAMES {
        run_audio_frame(&mut app);
    }
    let stats = collect_prepared_performance_stats(MEASUREMENT_FRAMES, || {
        prepare_audio_frame(&mut app);
        let sample = measure_performance_sample(|| app.update());
        cleanup_audio_frame(&mut app);
        sample
    });
    print_performance_stats("audio_frame_budget", stats);
    assert_performance_budget("audio frame", stats, AUDIO_BUDGET);
}

/// Measures snapshot comparison and retained-node synchronization with every
/// major modal populated. Initial spawning is excluded by the warmup.
#[test]
#[ignore]
fn retained_ui_frame_p99_hitch_and_allocation_budget() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut app = full_app_fixture();
    app.add_schedule(Schedule::new(UiBenchmark));
    app.add_systems(
        UiBenchmark,
        (
            sync_audio_settings_window,
            sync_enemy_settings_window,
            sync_technology_panel,
            sync_manual_crafting_panel,
            sync_production_stats_window,
            sync_objectives_panel,
            sync_threat_ui,
        )
            .chain(),
    );

    app.update();
    open_benchmark_windows(&mut app);
    for _ in 0..WARMUP_FRAMES {
        app.world_mut().run_schedule(UiBenchmark);
    }

    let stats = collect_performance_stats(MEASUREMENT_FRAMES, || {
        app.world_mut().run_schedule(UiBenchmark);
    });
    print_performance_stats("retained_ui_frame_budget", stats);
    assert_performance_budget("retained UI frame", stats, UI_BUDGET);
}

/// Covers the complete headless CPU frame: time advancement, fixed-step
/// simulation, input, audio, UI, map updates, and presentation synchronization.
#[test]
#[ignore]
fn full_app_frame_p99_hitch_and_allocation_budget() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut app = full_app_fixture();
    app.update();
    open_benchmark_windows(&mut app);

    for _ in 0..WARMUP_FRAMES {
        app.update();
    }
    let stats = collect_performance_stats(MEASUREMENT_FRAMES, || app.update());
    print_performance_stats("full_app_frame_budget", stats);
    assert_performance_budget("full app frame", stats, FULL_FRAME_BUDGET);

    app.world()
        .resource::<SimResource>()
        .read()
        .validate_state()
        .expect("full-frame budget should retain a valid simulation");
}

/// Same complete frame as above, with a sky full of robots on top: the flight
/// step, the interpolated sprite sync, and the roboport charging all run inside
/// the one 60 fps frame budget rather than getting a budget of their own.
#[test]
#[ignore]
fn robot_heavy_frame_p99_hitch_and_allocation_budget() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut app = robot_benchmark_app();
    app.update();

    for _ in 0..WARMUP_FRAMES {
        app.update();
    }
    let stats = collect_performance_stats(MEASUREMENT_FRAMES, || app.update());
    print_performance_stats("robot_heavy_frame_budget", stats);
    assert_performance_budget("robot-heavy app frame", stats, FULL_FRAME_BUDGET);

    let drawn = {
        let world = app.world_mut();
        let mut sprites = world.query::<&RobotSprite>();
        sprites.iter(world).count()
    };
    let sim = app.world().resource::<SimResource>().read();
    assert!(
        sim.robot_count() >= BENCHMARK_ROBOTS / 2,
        "the benchmark should still be flying robots at the end, found {}",
        sim.robot_count()
    );
    // The frame is only worth measuring if the presentation half ran too: a
    // budget met by drawing nothing would say nothing about robot rendering.
    assert!(
        drawn > 0,
        "the benchmark frame should be drawing robots, not only simulating them"
    );
    sim.validate_state()
        .expect("robot-heavy budget run should retain a valid simulation");
}

/// The same complete frame as the full-app benchmark, with a railway full of
/// driving trains on top: the per-tick rail walk, the fuel burn, and the
/// interpolated body sync all run inside the one 60 fps frame budget rather
/// than getting a budget of their own.
#[test]
#[ignore]
fn train_heavy_frame_p99_hitch_and_allocation_budget() {
    let _guard = BENCHMARK_LOCK
        .lock()
        .expect("benchmark lock should not poison");
    let mut app = train_benchmark_app();
    app.update();

    for _ in 0..WARMUP_FRAMES {
        app.update();
    }
    let stats = collect_performance_stats(MEASUREMENT_FRAMES, || app.update());
    print_performance_stats("train_heavy_frame_budget", stats);
    assert_performance_budget("train-heavy app frame", stats, FULL_FRAME_BUDGET);

    let drawn = {
        let world = app.world_mut();
        let mut sprites = world.query::<&RollingStockSprite>();
        sprites.iter(world).count()
    };
    let sim = app.world().resource::<SimResource>().read();
    assert!(
        sim.train_count() >= BENCHMARK_TRAINS / 2,
        "the benchmark should still be running trains at the end, found {}",
        sim.train_count()
    );
    // The frame is only worth measuring if the presentation half ran too: a
    // budget met by drawing nothing would say nothing about train rendering.
    assert!(
        drawn > 0,
        "the benchmark frame should be drawing trains, not only simulating them"
    );
    sim.validate_state()
        .expect("train-heavy budget run should retain a valid simulation");
}

fn train_benchmark_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    let mut sim = Simulation::new_rolling_stock_fixture(BENCHMARK_TRAINS);
    sim.tick();
    app.world_mut()
        .resource_mut::<SimResource>()
        .replace(sim)
        .expect("benchmark simulation should replace before frame execution");
    app
}

fn robot_benchmark_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    let mut sim = Simulation::new_robot_flight_fixture(BENCHMARK_ROBOTS);
    sim.tick();
    app.world_mut()
        .resource_mut::<SimResource>()
        .replace(sim)
        .expect("benchmark simulation should replace before frame execution");
    app
}

fn audio_benchmark_app() -> App {
    let sim = Simulation::new_scripted_red_science_factory();
    let visible_ids = sim
        .entities()
        .placed_entities()
        .map(|placed| placed.id)
        .collect();
    let handle = Handle::<AudioSource>::default();
    let assets = AudioAssets {
        ui_click: Some(handle.clone()),
        place: Some(handle.clone()),
        place_error: Some(handle.clone()),
        manual_mine_tick: Some(handle.clone()),
        manual_mine_complete: Some(handle.clone()),
        craft_complete: Some(handle.clone()),
        machine_burner_loop: Some(handle.clone()),
        machine_electric_loop: Some(handle.clone()),
        research_complete: Some(handle.clone()),
        enemy_warning: Some(handle),
    };

    let mut app = App::new();
    app.insert_resource(SimResource::new(sim))
        .insert_resource(VisibleEntityIds {
            ids: visible_ids,
            visible_revision: 1,
            entity_topology_revision: 1,
        })
        .insert_resource(assets)
        .init_resource::<AudioSettings>()
        .init_resource::<AudioEventDedupe>()
        .init_resource::<MachineAudioLoops>()
        .add_message::<SoundEvent>()
        .add_systems(
            Update,
            (sync_machine_audio_loops, play_sound_events).chain(),
        );
    app
}

fn run_audio_frame(app: &mut App) {
    prepare_audio_frame(app);
    app.update();
    cleanup_audio_frame(app);
}

fn prepare_audio_frame(app: &mut App) {
    app.world_mut()
        .resource_mut::<SimResource>()
        .write_for_tests()
        .tick();
    for _ in 0..AUDIO_EVENTS_PER_FRAME {
        app.world_mut().write_message(SoundEvent::Place);
    }
}

fn cleanup_audio_frame(app: &mut App) {
    let audio_players = {
        let world = app.world_mut();
        let mut query = world.query_filtered::<Entity, With<AudioPlayer<AudioSource>>>();
        query.iter(world).collect::<Vec<_>>()
    };
    for entity in audio_players {
        app.world_mut().despawn(entity);
    }
}

fn full_app_fixture() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    let mut sim = Simulation::new_scripted_red_science_factory();
    sim.tick();
    app.world_mut()
        .resource_mut::<SimResource>()
        .replace(sim)
        .expect("benchmark simulation should replace before frame execution");
    app
}

fn open_benchmark_windows(app: &mut App) {
    app.world_mut()
        .resource_mut::<AudioSettingsWindowState>()
        .open = true;
    app.world_mut()
        .resource_mut::<EnemySettingsWindowState>()
        .open = true;
    app.world_mut().resource_mut::<TechnologyWindowState>().open = true;
    app.world_mut().resource_mut::<CraftingWindowState>().open = true;
    app.world_mut()
        .resource_mut::<ProductionStatsWindowState>()
        .open = true;
}
