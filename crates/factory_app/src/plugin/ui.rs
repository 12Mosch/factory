use bevy::asset::{AssetId, Assets};
use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::input::build::handle_build_world_click;
use crate::interaction::command_feedback::{
    ItemGainFeedback, expire_item_gain_feedback, handle_sim_command_results,
};
use crate::interaction::container_open::{
    handle_container_close_input, handle_container_open_input,
};
use crate::resources::UpsStats;
use crate::ui::assembler_panel::{
    handle_assembler_recipe_button_clicks, update_assembler_detail_text,
    update_assembler_recipe_button_colors,
};
use crate::ui::build_menu::handle_build_menu_buttons;
use crate::ui::container_window::sync_container_window;
use crate::ui::debug_overlay::{
    DebugOverlayVisible, apply_debug_overlay_visibility, setup_debug_overlay, toggle_debug_overlay,
    update_debug_overlay, update_ups_stats,
};
use crate::ui::enemy_settings::{
    EnemySettingsWindowState, handle_enemy_settings_buttons, sync_enemy_settings_window,
};
use crate::ui::inventory_panel::{
    handle_container_slot_clicks, update_container_slot_text,
    update_inventory_transfer_feedback_text,
};
use crate::ui::machine_indicators::{update_machine_guidance, update_machine_indicators};
use crate::ui::manual_crafting::{
    handle_manual_crafting_recipe_buttons, handle_manual_crafting_tab_buttons,
    sync_manual_crafting_panel,
};
use crate::ui::objectives_panel::{
    ObjectivesPanelState, setup_objectives_panel, sync_objectives_panel,
};
use crate::ui::production_stats::{handle_production_stats_buttons, sync_production_stats_window};
use crate::ui::resources::{
    CraftingWindowState, InventoryTransferFeedback, OpenContainer, ProductionStatsWindowState,
    TechnologyWindowState,
};
use crate::ui::technology_panel::{
    ensure_selected_technology, handle_technology_panel_buttons, handle_technology_window_input,
    sync_technology_panel,
};
use crate::ui::threat::{
    ThreatUiState, handle_threat_alert_clicks, setup_threat_ui, sync_threat_ui,
};

/// General UI: debug overlay, containers and inventory, technology window,
/// manual crafting, production stats, and machine indicators.
pub(super) struct UiPlugin;

const DEFAULT_UI_FONT: &[u8] = include_bytes!("../../third_party/fira_mono/FiraMono-Medium.ttf");

/// Replaces Bevy's ASCII-only Fira Mono subset while retaining its default
/// font handle, so constructors such as `TextFont::from_font_size` use the
/// complete face without requiring per-widget font wiring.
fn install_default_ui_font(app: &mut App) {
    if !app.world().contains_resource::<Assets<Font>>() {
        // Headless tests use MinimalPlugins and intentionally have no text assets.
        return;
    }

    // Bevy 0.19's TextPlugin registers DEFAULT_FONT_DATA at AssetId::default().
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    fonts
        .insert(
            AssetId::default(),
            Font::from_bytes(DEFAULT_UI_FONT.to_vec()),
        )
        .expect("the default font asset ID must remain valid");
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        install_default_ui_font(app);

        app.init_resource::<UpsStats>()
            .init_resource::<DebugOverlayVisible>()
            .init_resource::<OpenContainer>()
            .init_resource::<InventoryTransferFeedback>()
            .init_resource::<ItemGainFeedback>()
            .init_resource::<TechnologyWindowState>()
            .init_resource::<CraftingWindowState>()
            .init_resource::<ProductionStatsWindowState>()
            .init_resource::<ObjectivesPanelState>()
            .init_resource::<EnemySettingsWindowState>()
            .init_resource::<ThreatUiState>()
            .add_systems(
                Startup,
                (setup_debug_overlay, setup_objectives_panel, setup_threat_ui),
            )
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
                    toggle_debug_overlay,
                    handle_container_open_input.before(handle_build_world_click),
                    handle_container_close_input,
                )
                    .in_set(AppSet::WorldInput),
            )
            .add_systems(
                Update,
                (
                    apply_debug_overlay_visibility.after(toggle_debug_overlay),
                    update_debug_overlay,
                    // The menu clears `open_container` when it opens; sync after
                    // it so the container window hides on the same frame.
                    sync_container_window.after(handle_build_menu_buttons),
                    handle_container_slot_clicks.in_set(AppSet::UiInteraction),
                    handle_sim_command_results.in_set(AppSet::UiInteraction),
                    expire_item_gain_feedback.after(handle_sim_command_results),
                    update_container_slot_text,
                    update_inventory_transfer_feedback_text
                        .after(sync_container_window)
                        .after(handle_container_slot_clicks)
                        .after(handle_sim_command_results),
                    update_machine_indicators,
                    update_machine_guidance.after(sync_container_window),
                    sync_objectives_panel,
                    handle_threat_alert_clicks.in_set(AppSet::UiInteraction),
                    sync_threat_ui.after(handle_threat_alert_clicks),
                    handle_enemy_settings_buttons.in_set(AppSet::UiInteraction),
                    sync_enemy_settings_window.after(handle_enemy_settings_buttons),
                    handle_production_stats_buttons.in_set(AppSet::UiInteraction),
                    sync_production_stats_window.after(handle_production_stats_buttons),
                    handle_manual_crafting_tab_buttons.in_set(AppSet::UiInteraction),
                    handle_manual_crafting_recipe_buttons.in_set(AppSet::UiInteraction),
                    sync_manual_crafting_panel
                        .after(handle_manual_crafting_tab_buttons)
                        .after(handle_manual_crafting_recipe_buttons),
                )
                    .in_set(InGameSet),
            )
            .add_systems(
                Update,
                (
                    handle_assembler_recipe_button_clicks
                        .in_set(AppSet::UiInteraction)
                        .after(sync_container_window),
                    update_assembler_detail_text.after(sync_container_window),
                    update_assembler_recipe_button_colors.after(sync_container_window),
                )
                    .in_set(InGameSet),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::text::TextPlugin;

    #[test]
    fn replaces_text_plugins_subset_at_the_default_font_handle() {
        let mut app = App::new();
        app.add_plugins((AssetPlugin::default(), TextPlugin));

        let subset = app
            .world()
            .resource::<Assets<Font>>()
            .get(AssetId::default())
            .expect("TextPlugin should install Bevy's default font subset");
        assert_ne!(subset.data.data(), DEFAULT_UI_FONT);

        install_default_ui_font(&mut app);

        let fonts = app.world().resource::<Assets<Font>>();
        let font = fonts
            .get(AssetId::default())
            .expect("the complete UI font should occupy Bevy's default handle");
        assert_eq!(font.data.data(), DEFAULT_UI_FONT);
    }
}
