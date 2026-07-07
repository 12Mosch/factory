use bevy::prelude::*;

use super::AppSet;
use crate::input::build::handle_build_world_click;
use crate::interaction::container_open::{
    handle_container_close_input, handle_container_open_input,
};
use crate::resources::{
    CraftingWindowState, InventoryTransferFeedback, OpenContainer, ProductionStatsWindowState,
    TechnologyWindowState, UpsStats,
};
use crate::ui::assembler_panel::{
    handle_assembler_recipe_button_clicks, update_assembler_detail_text,
    update_assembler_recipe_button_colors,
};
use crate::ui::build_menu::handle_build_menu_buttons;
use crate::ui::container_window::sync_container_window;
use crate::ui::debug_overlay::{setup_debug_overlay, update_debug_overlay, update_ups_stats};
use crate::ui::inventory_panel::{
    handle_container_slot_clicks, update_container_slot_text,
    update_inventory_transfer_feedback_text,
};
use crate::ui::machine_indicators::update_burner_drill_indicators;
use crate::ui::manual_crafting::{
    handle_manual_crafting_recipe_buttons, handle_manual_crafting_tab_buttons,
    sync_manual_crafting_panel,
};
use crate::ui::production_stats::{handle_production_stats_buttons, sync_production_stats_window};
use crate::ui::technology_panel::{
    ensure_selected_technology, handle_technology_panel_buttons, handle_technology_window_input,
    sync_technology_panel,
};

/// General UI: debug overlay, containers and inventory, technology window,
/// manual crafting, production stats, and machine indicators.
pub(super) struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UpsStats>()
            .init_resource::<OpenContainer>()
            .init_resource::<InventoryTransferFeedback>()
            .init_resource::<TechnologyWindowState>()
            .init_resource::<CraftingWindowState>()
            .init_resource::<ProductionStatsWindowState>()
            .add_systems(Startup, setup_debug_overlay)
            .add_systems(
                Update,
                (
                    handle_technology_window_input,
                    ensure_selected_technology,
                    handle_technology_panel_buttons.in_set(AppSet::UiInteraction),
                    sync_technology_panel,
                )
                    .chain()
                    .in_set(AppSet::TechnologyWindow),
            )
            .add_systems(
                Update,
                (
                    update_ups_stats,
                    handle_container_open_input.before(handle_build_world_click),
                    handle_container_close_input,
                )
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    update_debug_overlay,
                    // The menu clears `open_container` when it opens; sync after
                    // it so the container window hides on the same frame.
                    sync_container_window.after(handle_build_menu_buttons),
                    handle_container_slot_clicks.in_set(AppSet::UiInteraction),
                    update_container_slot_text,
                    update_inventory_transfer_feedback_text
                        .after(sync_container_window)
                        .after(handle_container_slot_clicks),
                    update_burner_drill_indicators,
                    handle_production_stats_buttons.in_set(AppSet::UiInteraction),
                    sync_production_stats_window.after(handle_production_stats_buttons),
                    handle_manual_crafting_tab_buttons.in_set(AppSet::UiInteraction),
                    handle_manual_crafting_recipe_buttons.in_set(AppSet::UiInteraction),
                    sync_manual_crafting_panel
                        .after(handle_manual_crafting_tab_buttons)
                        .after(handle_manual_crafting_recipe_buttons),
                ),
            )
            .add_systems(
                Update,
                (
                    handle_assembler_recipe_button_clicks
                        .in_set(AppSet::UiInteraction)
                        .after(sync_container_window),
                    update_assembler_detail_text.after(sync_container_window),
                    update_assembler_recipe_button_colors.after(sync_container_window),
                ),
            );
    }
}
