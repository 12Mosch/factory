use bevy::prelude::*;
use factory_sim::Simulation;

pub struct FactoryAppPlugin;

#[derive(Resource)]
pub struct SimResource {
    pub sim: Simulation,
}

impl Plugin for FactoryAppPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SimResource {
            sim: Simulation::new_test_world(123),
        })
        .add_systems(Startup, setup_camera)
        .add_systems(FixedUpdate, tick_sim);
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

fn tick_sim(mut sim: ResMut<SimResource>) {
    sim.sim.tick();
}
