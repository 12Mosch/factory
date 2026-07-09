mod audio;
mod build;
mod construction;
mod input;
mod map;
mod rendering;
mod save_load;
mod simulation;
mod ui;

use bevy::diagnostic::{DiagnosticsPlugin, FrameCountPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::InputSystems;
use bevy::prelude::*;

/// Shared ordering labels for systems whose ordering constraints cross plugin
/// boundaries. Plugin-internal ordering uses direct `.before`/`.after` edges
/// instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub(crate) enum AppSet {
    /// `PreUpdate`: panel open/close input; populates `AppInputState` for the
    /// rest of the frame.
    PanelInput,
    /// `FixedUpdate`: systems that turn frame-collected input into queued
    /// `SimCommandRequest`s for this tick.
    SimInput,
    /// `FixedUpdate`: drains the queued commands into the simulation, then
    /// runs the simulation tick itself.
    SimTick,
    /// `FixedUpdate`: systems that observe simulation state changes produced
    /// by the tick (e.g. audio observers).
    PostTick,
    /// `Update`: technology window input and sync; runs before `WorldInput`
    /// so an open window blocks world interaction on the same frame.
    TechnologyWindow,
    /// `Update`: world interaction and presentation gated on panel state
    /// (build placement, container open/close, camera, cursor).
    WorldInput,
    /// `Update`: interaction handlers that may emit `SoundEvent`s. Any new
    /// button-click or click-to-act system belongs in this set so
    /// `play_sound_events` picks its sounds up on the same frame.
    UiInteraction,
    /// `Update`: map texture regeneration. State changes that must be visible
    /// to rendering this frame (e.g. a loaded save) run `.before` this set.
    MapTexture,
    /// `Update`: the chained render-sync systems (render detail, visible
    /// chunks/entities, tiles, entities, belts).
    RenderSync,
    /// `Update`: marks `update_visible_entity_ids` inside `RenderSync` so
    /// consumers of fresh visibility data can order after it alone.
    VisibleEntities,
}

pub struct FactoryAppPlugin;

impl Plugin for FactoryAppPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }
        if !app.is_plugin_added::<FrameCountPlugin>() {
            app.add_plugins(FrameCountPlugin);
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }

        app.configure_sets(PreUpdate, AppSet::PanelInput.after(InputSystems))
            .configure_sets(
                FixedUpdate,
                (AppSet::SimInput, AppSet::SimTick, AppSet::PostTick).chain(),
            )
            .configure_sets(
                Update,
                (AppSet::TechnologyWindow, AppSet::WorldInput).chain(),
            )
            .configure_sets(Update, (AppSet::MapTexture, AppSet::RenderSync).chain());

        // SimulationPlugin must come first: BuildPlugin reads the prototype
        // catalog from `SimResource` to build the default hotbar.
        app.add_plugins((
            simulation::SimulationPlugin,
            input::InputPlugin,
            audio::AudioPlugin,
            build::BuildPlugin,
            construction::ConstructionPlugin,
            map::MapPlugin,
            rendering::RenderingPlugin,
            save_load::SaveLoadPlugin,
            ui::UiPlugin,
        ));
    }
}
