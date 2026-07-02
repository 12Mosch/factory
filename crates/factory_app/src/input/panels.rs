use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::AudioSettingsWindowState;
use crate::resources::{
    AppInputState, BuildPlacementState, CraftingWindowState, MapDisplaySettings, MapViewState,
    OpenContainer, ProductionStatsWindowState, TechnologyWindowState,
};
use crate::save_load::SaveLoadWindowState;

pub(crate) fn reset_app_input_state(
    map: Res<MapViewState>,
    stats: Res<ProductionStatsWindowState>,
    crafting: Res<CraftingWindowState>,
    audio_settings: Res<AudioSettingsWindowState>,
    save_load: Res<SaveLoadWindowState>,
    mut input_state: ResMut<AppInputState>,
) {
    input_state.world_blocked =
        map.open || stats.open || crafting.open || audio_settings.open || save_load.open;
    input_state.escape_consumed = false;
}

#[derive(SystemParam)]
pub(crate) struct PanelInputResources<'w> {
    input_state: ResMut<'w, AppInputState>,
    map: ResMut<'w, MapViewState>,
    settings: ResMut<'w, MapDisplaySettings>,
    stats: ResMut<'w, ProductionStatsWindowState>,
    crafting: ResMut<'w, CraftingWindowState>,
    audio_settings: ResMut<'w, AudioSettingsWindowState>,
    technology: ResMut<'w, TechnologyWindowState>,
    save_load: ResMut<'w, SaveLoadWindowState>,
    open_container: ResMut<'w, OpenContainer>,
    build_state: ResMut<'w, BuildPlacementState>,
}

pub(crate) fn handle_panel_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut resources: PanelInputResources,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    if keyboard.just_pressed(KeyCode::KeyM) {
        resources.map.open = !resources.map.open;
        if resources.map.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    if keyboard.just_pressed(KeyCode::KeyP) {
        resources.stats.open = !resources.stats.open;
        if resources.stats.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    if keyboard.just_pressed(KeyCode::KeyC) {
        resources.crafting.open = !resources.crafting.open;
        if resources.crafting.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    if keyboard.just_pressed(KeyCode::KeyO) {
        resources.audio_settings.open = !resources.audio_settings.open;
        if resources.audio_settings.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    if keyboard.just_pressed(KeyCode::F3) {
        resources.settings.debug_reveal_all = !resources.settings.debug_reveal_all;
        resources.settings.show_chunk_grid = resources.settings.debug_reveal_all;
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        if resources.map.open {
            resources.map.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.stats.open {
            resources.stats.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.crafting.open {
            resources.crafting.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.audio_settings.open {
            resources.audio_settings.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.technology.open {
            resources.technology.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.open_container.entity_id.is_some() {
            resources.open_container.entity_id = None;
            resources.input_state.escape_consumed = true;
        } else if resources.build_state.selected.is_some() {
            resources.build_state.selected = None;
            resources.build_state.last_status = Default::default();
            resources.input_state.escape_consumed = true;
        } else if resources.save_load.open {
            resources.save_load.open = false;
            resources.input_state.escape_consumed = true;
        } else {
            resources.save_load.open = true;
            resources.save_load.tab = crate::save_load::SaveLoadTab::Save;
            resources.input_state.escape_consumed = true;
        }
    }

    resources.input_state.world_blocked = resources.map.open
        || resources.stats.open
        || resources.crafting.open
        || resources.audio_settings.open
        || resources.save_load.open;
}

pub fn world_input_blocked(input_state: Option<&AppInputState>) -> bool {
    input_state.is_some_and(|state| state.world_blocked)
}

pub fn escape_consumed(input_state: Option<&AppInputState>) -> bool {
    input_state.is_some_and(|state| state.escape_consumed)
}
