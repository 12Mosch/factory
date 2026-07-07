use bevy::prelude::*;

use super::AppSet;
use crate::save_load::{
    AutosaveState, PendingSaveJobs, PresentationReloadToken, SaveLoadConfig, SaveLoadStatus,
    SaveLoadWindowState, handle_save_load_shortcuts, initialize_autosave_tick, poll_save_jobs,
    run_autosave,
};
use crate::ui::save_load::{handle_save_load_buttons, sync_save_load_window};

/// Manual and automatic save/load, plus the save/load window.
pub(super) struct SaveLoadPlugin;

impl Plugin for SaveLoadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveLoadConfig>()
            .init_resource::<SaveLoadWindowState>()
            .init_resource::<SaveLoadStatus>()
            .init_resource::<PendingSaveJobs>()
            .init_resource::<AutosaveState>()
            .init_resource::<PresentationReloadToken>()
            .add_systems(Startup, initialize_autosave_tick)
            .add_systems(
                Update,
                (
                    handle_save_load_shortcuts,
                    handle_save_load_buttons.in_set(AppSet::UiInteraction),
                    run_autosave,
                    poll_save_jobs,
                    sync_save_load_window,
                )
                    .chain()
                    // A load applied by `poll_save_jobs` must be reflected by
                    // this frame's map texture and render sync.
                    .before(AppSet::MapTexture),
            );
    }
}
