use bevy::prelude::*;
use bevy::time::Fixed;
use factory_sim::Simulation;

pub const SIM_TICKS_PER_SECOND: f64 = 60.0;

pub struct FactoryAppPlugin;

#[derive(Resource)]
pub struct SimResource {
    pub sim: Simulation,
}

#[derive(Resource, Default)]
pub struct UpsStats {
    elapsed: f64,
    fixed_ticks: u32,
    pub ups: f64,
}

#[derive(Component)]
struct DebugOverlayText;

impl Plugin for FactoryAppPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(SIM_TICKS_PER_SECOND))
            .insert_resource(SimResource {
                sim: Simulation::new_test_world(123),
            })
            .init_resource::<UpsStats>()
            .add_systems(Startup, (setup_camera, setup_debug_overlay))
            .add_systems(FixedUpdate, tick_sim)
            .add_systems(Update, (update_ups_stats, update_debug_overlay));
    }
}

pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(FactoryAppPlugin);
    app
}

pub fn run() {
    app().run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                left: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.72)),
            GlobalZIndex(1000),
        ))
        .with_child((
            Text::new("Tick: 0\nUPS: 0.0"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
            DebugOverlayText,
        ));
}

fn tick_sim(mut sim: ResMut<SimResource>, mut ups: ResMut<UpsStats>) {
    sim.sim.tick();
    ups.fixed_ticks += 1;
}

fn update_ups_stats(time: Res<Time<Real>>, mut stats: ResMut<UpsStats>) {
    let delta = time.delta_secs_f64();
    if delta <= 0.0 {
        return;
    }

    stats.elapsed += delta;
    if stats.elapsed >= 1.0 {
        stats.ups = f64::from(stats.fixed_ticks) / stats.elapsed;
        stats.elapsed = 0.0;
        stats.fixed_ticks = 0;
    }
}

fn update_debug_overlay(
    sim: Res<SimResource>,
    stats: Res<UpsStats>,
    mut overlay: Query<&mut Text, With<DebugOverlayText>>,
) {
    for mut text in &mut overlay {
        text.0 = format!("Tick: {}\nUPS: {:.1}", sim.sim.tick_count(), stats.ups);
    }
}
