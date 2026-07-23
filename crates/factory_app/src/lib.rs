mod app;
pub mod audio;
mod plugin;
pub mod save_load;
pub mod simulation;
mod threat_events;
mod utils;
pub mod world_setup;

pub mod build;
pub mod constants;
pub mod input;
pub mod interaction;
pub mod map;
pub mod placement;
pub mod rendering;
pub mod resources;
pub mod ui;

pub use app::{app, run};
pub use plugin::FactoryAppPlugin;

#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod test_performance;
