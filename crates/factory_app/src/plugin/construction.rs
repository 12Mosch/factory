use bevy::prelude::*;

use super::AppSet;
use crate::build::resources::{BlueprintLibraryWindowState, PlannerState};
use crate::input::build::handle_build_world_click;
use crate::input::planner::{
    handle_ghost_click, handle_paste_click, handle_planner_drag, handle_planner_keys,
};
use crate::rendering::construction::{
    ConstructionRenderState, spawn_planner_selection_rect, sync_construction_rendering,
    update_paste_preview, update_planner_selection_rect,
};
use crate::ui::blueprint_library::{
    handle_blueprint_library_buttons, sync_blueprint_library_window,
};

/// Construction planning: ghost placement, the deconstruction planner,
/// copy/paste, the blueprint library, and their world rendering.
pub(super) struct ConstructionPlugin;

impl Plugin for ConstructionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlannerState>()
            .init_resource::<BlueprintLibraryWindowState>()
            .init_resource::<ConstructionRenderState>()
            .add_systems(Startup, spawn_planner_selection_rect)
            .add_systems(
                Update,
                (
                    handle_planner_keys.before(handle_planner_drag),
                    handle_planner_drag.in_set(AppSet::UiInteraction),
                    handle_paste_click.in_set(AppSet::UiInteraction),
                    // Ghost clicks must not race the build click: an active
                    // selection wins, an empty cursor interacts with ghosts.
                    handle_ghost_click
                        .in_set(AppSet::UiInteraction)
                        .after(handle_build_world_click),
                    update_planner_selection_rect.after(handle_planner_drag),
                    update_paste_preview,
                )
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    handle_blueprint_library_buttons.in_set(AppSet::UiInteraction),
                    sync_blueprint_library_window.after(handle_blueprint_library_buttons),
                    sync_construction_rendering
                        .in_set(AppSet::RenderSync)
                        .after(AppSet::VisibleEntities),
                ),
            );
    }
}
