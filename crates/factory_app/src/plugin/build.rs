use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::build::resources::{
    BuildMenuState, BuildPlacementPreviewState, BuildPlacementState, HotbarState,
};
use crate::input::build::{
    handle_build_hotbar_keys, handle_build_rotate_cancel_keys, handle_build_world_click,
};
use crate::interaction::container_open::handle_container_close_input;
use crate::placement::build::default_hotbar_slots;
use crate::rendering::build_preview::{
    spawn_build_preview, update_build_placement_preview_state, update_build_preview,
};
use crate::rendering::construction::update_paste_preview;
use crate::resources::SimResource;
use crate::ui::build_bar::{
    handle_build_bar_button_clicks, setup_build_bar, update_build_bar_action_visuals,
    update_build_bar_visuals, update_build_status_text,
};
use crate::ui::build_menu::{handle_build_menu_buttons, sync_build_menu};

/// Build placement input, preview, hotbar, build bar, and build menu.
pub(super) struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        let hotbar = HotbarState {
            slots: default_hotbar_slots(app.world().resource::<SimResource>().read().catalog()),
        };

        app.insert_resource(hotbar)
            .init_resource::<BuildPlacementState>()
            .init_resource::<BuildPlacementPreviewState>()
            .init_resource::<BuildMenuState>()
            .add_systems(Startup, (setup_build_bar, spawn_build_preview))
            .add_systems(
                Update,
                (
                    handle_build_hotbar_keys,
                    handle_build_rotate_cancel_keys.before(handle_container_close_input),
                    handle_build_bar_button_clicks.in_set(AppSet::UiInteraction),
                    handle_build_world_click.in_set(AppSet::UiInteraction),
                    update_build_placement_preview_state.before(update_build_preview),
                    update_build_preview,
                )
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    update_build_bar_visuals,
                    update_build_bar_action_visuals,
                    update_build_status_text
                        .after(update_build_placement_preview_state)
                        .after(update_paste_preview),
                    handle_build_menu_buttons.in_set(AppSet::UiInteraction),
                    sync_build_menu.after(handle_build_menu_buttons),
                )
                    .in_set(InGameSet),
            );
    }
}
