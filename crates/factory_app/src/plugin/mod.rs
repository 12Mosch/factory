mod audio;
mod build;
mod construction;
mod input;
mod map;
mod rendering;
mod save_load;
mod simulation;
mod ui;

use crate::world_setup::{
    AppMode, StartInWorldSetup, WorldSetupSaveListState, build_world_setup_ui, cleanup_world_setup,
    handle_world_setup_buttons, handle_world_setup_load_buttons, handle_world_setup_seed_input,
    sync_world_setup_save_list, sync_world_setup_text,
};
use bevy::diagnostic::{DiagnosticsPlugin, FrameCountPlugin, FrameTimeDiagnosticsPlugin};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

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

/// Umbrella set for systems that only make sense while a world is being
/// played. It carries the `in_state(AppMode::InGame)` run condition in every
/// schedule it is configured for, so gameplay systems are gated in one place:
/// membership (direct or via an [`AppSet`] configured into it) is what keeps
/// a system from running on the world-setup screen. New in-game systems
/// should join it (or one of its member sets) rather than adding their own
/// `run_if`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub(crate) struct InGameSet;

pub struct FactoryAppPlugin;

impl Plugin for FactoryAppPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }
        let initial_mode = if app.world().contains_resource::<StartInWorldSetup>() {
            AppMode::WorldSetup
        } else {
            AppMode::InGame
        };
        app.insert_state(initial_mode);
        if !app.is_plugin_added::<DiagnosticsPlugin>() {
            app.add_plugins(DiagnosticsPlugin);
        }
        if !app.is_plugin_added::<FrameCountPlugin>() {
            app.add_plugins(FrameCountPlugin);
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }

        app.configure_sets(PreUpdate, InGameSet.run_if(in_state(AppMode::InGame)))
            .configure_sets(FixedUpdate, InGameSet.run_if(in_state(AppMode::InGame)))
            .configure_sets(Update, InGameSet.run_if(in_state(AppMode::InGame)))
            .configure_sets(
                PreUpdate,
                AppSet::PanelInput.after(InputSystems).in_set(InGameSet),
            )
            .configure_sets(
                FixedUpdate,
                (AppSet::SimInput, AppSet::SimTick, AppSet::PostTick)
                    .chain()
                    .in_set(InGameSet),
            )
            .configure_sets(
                Update,
                (AppSet::TechnologyWindow, AppSet::WorldInput)
                    .chain()
                    .in_set(InGameSet),
            )
            .configure_sets(Update, AppSet::UiInteraction.in_set(InGameSet))
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
        ))
        .init_resource::<crate::world_setup::WorldSetupState>()
        .init_resource::<WorldSetupSaveListState>()
        .add_systems(OnEnter(AppMode::WorldSetup), build_world_setup_ui)
        .add_systems(OnExit(AppMode::WorldSetup), cleanup_world_setup)
        .add_systems(
            Update,
            (
                handle_world_setup_seed_input,
                handle_world_setup_load_buttons,
                handle_world_setup_buttons,
                sync_world_setup_save_list,
                sync_world_setup_text,
            )
                .chain()
                .run_if(in_state(AppMode::WorldSetup)),
        );
    }
}
