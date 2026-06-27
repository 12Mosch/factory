use bevy::prelude::*;

use crate::plugin::FactoryAppPlugin;

pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(FactoryAppPlugin);
    app
}

pub fn run() {
    app().run();
}
