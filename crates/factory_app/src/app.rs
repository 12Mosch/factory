use bevy::prelude::*;

use crate::plugin::FactoryAppPlugin;
use crate::world_setup::StartInWorldSetup;

pub fn app() -> App {
    let mut app = App::new();
    app.insert_resource(StartInWorldSetup)
        .add_plugins(DefaultPlugins)
        .add_plugins(FactoryAppPlugin);
    app
}

pub fn run() {
    app().run();
}
