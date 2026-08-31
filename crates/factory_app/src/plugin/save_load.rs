use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::save_load::{
    AutosaveState, PendingSaveConfirmation, PendingSaveJobs, PresentationReloadToken, SaveCatalog,
    SaveLoadConfig, SaveLoadMetrics, SaveLoadStatus, SaveLoadWindowState,
    handle_save_load_shortcuts, initialize_save_state, poll_save_jobs,
    refresh_catalog_on_manager_open, run_autosave,
};
use crate::ui::save_load::{
    SaveCreateRequested, handle_save_load_buttons, submit_save_create_requests,
    submit_save_name_input, sync_save_load_window, sync_save_name_from_state,
    sync_save_name_to_state,
};
use crate::ui::text_input::TextInputSanitization;

/// Manual and automatic save/load, plus the save/load window.
pub(super) struct SaveLoadPlugin;

impl Plugin for SaveLoadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveLoadConfig>()
            .init_resource::<SaveLoadWindowState>()
            .init_resource::<SaveLoadStatus>()
            .init_resource::<SaveCatalog>()
            .init_resource::<PendingSaveConfirmation>()
            .init_resource::<SaveLoadMetrics>()
            .init_resource::<PendingSaveJobs>()
            .init_resource::<AutosaveState>()
            .init_resource::<PresentationReloadToken>()
            .add_message::<SaveCreateRequested>()
            .add_systems(Startup, initialize_save_state)
            .add_systems(
                Update,
                (
                    handle_save_load_shortcuts.in_set(InGameSet),
                    sync_save_name_from_state.in_set(InGameSet),
                    handle_save_load_buttons.in_set(AppSet::UiInteraction),
                    run_autosave.in_set(InGameSet),
                    // Save workers finish on their own thread; keep joining
                    // and reporting them even on the world-setup screen.
                    poll_save_jobs,
                    refresh_catalog_on_manager_open.in_set(InGameSet),
                    sync_save_load_window.in_set(InGameSet),
                )
                    .chain()
                    // A load applied by `poll_save_jobs` must be reflected by
                    // this frame's map texture and render sync.
                    .before(AppSet::MapTexture),
            )
            .add_systems(
                PostUpdate,
                (
                    sync_save_name_to_state,
                    submit_save_create_requests,
                    submit_save_name_input,
                )
                    .chain()
                    .after(TextInputSanitization)
                    .in_set(InGameSet),
            );
    }
}
