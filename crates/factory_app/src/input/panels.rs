use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::audio::AudioSettingsWindowState;
use crate::build::resources::{
    BlueprintLibraryWindowState, BuildMenuState, BuildPlacementState, PlannerState, PlannerTool,
};
use crate::input::resources::AppInputState;
use crate::map::resources::{MapDisplaySettings, MapLayer, MapTextureCache, MapViewState};
use crate::resources::SimResource;
use crate::save_load::SaveLoadWindowState;
use crate::simulation::SimCommandRequest;
use crate::ui::enemy_settings::EnemySettingsWindowState;
use crate::ui::map_view::{
    FULL_MAP_MAX_ZOOM, FULL_MAP_MIN_ZOOM, clamp_map_center, fullscreen_crop_bounds,
    fullscreen_map_display_size, fullscreen_map_image_size,
};
use crate::ui::resources::{
    CraftingWindowState, OpenContainer, ProductionStatsWindowState, TechnologyWindowState,
};
use factory_sim::SimCommand;

#[derive(SystemParam)]
pub(crate) struct WorldBlockingWindows<'w> {
    map: Res<'w, MapViewState>,
    stats: Res<'w, ProductionStatsWindowState>,
    crafting: Res<'w, CraftingWindowState>,
    audio_settings: Res<'w, AudioSettingsWindowState>,
    enemy_settings: Res<'w, EnemySettingsWindowState>,
    save_load: Res<'w, SaveLoadWindowState>,
    build_menu: Res<'w, BuildMenuState>,
    blueprint_library: Res<'w, BlueprintLibraryWindowState>,
}

impl WorldBlockingWindows<'_> {
    fn any_open(&self) -> bool {
        world_blocking_windows_open(
            self.map.open,
            self.stats.open,
            self.crafting.open,
            self.audio_settings.open,
            self.enemy_settings.open,
            self.save_load.open,
            self.build_menu.open,
            self.blueprint_library.open,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn world_blocking_windows_open(
    map_open: bool,
    stats_open: bool,
    crafting_open: bool,
    audio_settings_open: bool,
    enemy_settings_open: bool,
    save_load_open: bool,
    build_menu_open: bool,
    blueprint_library_open: bool,
) -> bool {
    map_open
        || stats_open
        || crafting_open
        || audio_settings_open
        || enemy_settings_open
        || save_load_open
        || build_menu_open
        || blueprint_library_open
}

pub(crate) fn reset_app_input_state(
    windows: WorldBlockingWindows,
    mut input_state: ResMut<AppInputState>,
) {
    input_state.world_blocked = windows.any_open();
    input_state.escape_consumed = false;
}

#[derive(SystemParam)]
pub(crate) struct PanelInputResources<'w> {
    input_state: ResMut<'w, AppInputState>,
    sim: Res<'w, SimResource>,
    map: ResMut<'w, MapViewState>,
    settings: ResMut<'w, MapDisplaySettings>,
    stats: ResMut<'w, ProductionStatsWindowState>,
    crafting: ResMut<'w, CraftingWindowState>,
    audio_settings: ResMut<'w, AudioSettingsWindowState>,
    enemy_settings: ResMut<'w, EnemySettingsWindowState>,
    technology: ResMut<'w, TechnologyWindowState>,
    save_load: ResMut<'w, SaveLoadWindowState>,
    build_menu: ResMut<'w, BuildMenuState>,
    open_container: ResMut<'w, OpenContainer>,
    build_state: ResMut<'w, BuildPlacementState>,
    planner: ResMut<'w, PlannerState>,
    blueprint_library: ResMut<'w, BlueprintLibraryWindowState>,
}

pub(crate) fn handle_panel_input(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut resources: PanelInputResources,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    if resources.build_menu.open {
        if keyboard.just_pressed(KeyCode::Escape) {
            if resources.build_menu.search_query.is_empty() {
                resources.build_menu.close();
            } else {
                resources.build_menu.search_query.clear();
                resources.build_menu.message = None;
            }
            resources.input_state.escape_consumed = true;
        }
        resources.input_state.world_blocked = world_blocking_windows_open(
            resources.map.open,
            resources.stats.open,
            resources.crafting.open,
            resources.audio_settings.open,
            resources.enemy_settings.open,
            resources.save_load.open,
            resources.build_menu.open,
            resources.blueprint_library.open,
        );
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyM) {
        resources.map.open = !resources.map.open;
        if resources.map.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
            if resources.map.follow_player {
                let (x, y) = resources.sim.read().player().position_tiles();
                resources.map.center_tile = Vec2::new(x, y);
            }
        }
    }
    if keyboard.just_pressed(KeyCode::KeyP) {
        resources.stats.open = !resources.stats.open;
        if resources.stats.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    let control_held =
        keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
    if keyboard.just_pressed(KeyCode::KeyC) && !control_held {
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
    if keyboard.just_pressed(KeyCode::KeyN) {
        resources.enemy_settings.open = !resources.enemy_settings.open;
        if resources.enemy_settings.open {
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    if keyboard.just_pressed(KeyCode::KeyB) && control_held {
        if resources.blueprint_library.open {
            resources.blueprint_library.close();
        } else {
            resources.blueprint_library.open = true;
            resources.build_state.selected = None;
            resources.open_container.entity_id = None;
        }
    }
    if keyboard.just_pressed(KeyCode::KeyB) && !control_held {
        resources.build_menu.open_fresh();
        resources.build_state.selected = None;
        resources.open_container.entity_id = None;
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
        } else if resources.enemy_settings.open {
            resources.enemy_settings.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.technology.open {
            resources.technology.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.blueprint_library.open {
            if resources.blueprint_library.editing_index.is_some() {
                resources.blueprint_library.cancel_rename();
            } else {
                resources.blueprint_library.close();
            }
            resources.input_state.escape_consumed = true;
        } else if resources.open_container.entity_id.is_some() {
            resources.open_container.entity_id = None;
            resources.input_state.escape_consumed = true;
        } else if resources.planner.tool != PlannerTool::None {
            resources.planner.set_tool(PlannerTool::None);
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

    resources.input_state.world_blocked = world_blocking_windows_open(
        resources.map.open,
        resources.stats.open,
        resources.crafting.open,
        resources.audio_settings.open,
        resources.enemy_settings.open,
        resources.save_load.open,
        resources.build_menu.open,
        resources.blueprint_library.open,
    );
}

pub(crate) fn handle_build_menu_search_input(
    mut inputs: MessageReader<KeyboardInput>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut menu: ResMut<BuildMenuState>,
) {
    if !menu.open {
        return;
    }
    let control_held = keyboard.as_deref().is_some_and(|keys| {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    });
    for input in inputs.read() {
        if input.state != ButtonState::Pressed || input.key_code == KeyCode::Escape {
            continue;
        }
        if input.key_code == KeyCode::Backspace {
            if control_held {
                remove_previous_word(&mut menu.search_query);
            } else {
                menu.search_query.pop();
            }
            menu.message = None;
            continue;
        }
        if control_held {
            continue;
        }
        if let Some(text) = &input.text {
            menu.search_query
                .extend(text.chars().filter(|character| !character.is_control()));
            menu.message = None;
        }
    }
}

/// Text entry for the blueprint library's in-progress rename. Escape is left
/// unhandled here; it is handled by [`handle_panel_input`]'s escape chain so
/// cancelling a rename cooperates with the rest of the window priorities.
pub(crate) fn handle_blueprint_rename_input(
    mut inputs: MessageReader<KeyboardInput>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mut window: ResMut<BlueprintLibraryWindowState>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(index) = window.editing_index else {
        return;
    };
    let control_held = keyboard.as_deref().is_some_and(|keys| {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    });
    for input in inputs.read() {
        if input.state != ButtonState::Pressed || input.key_code == KeyCode::Escape {
            continue;
        }
        match input.key_code {
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let name = window.rename_buffer.trim().to_string();
                if !name.is_empty() {
                    commands.write(SimCommandRequest(SimCommand::RenameBlueprint {
                        index,
                        name,
                    }));
                }
                window.cancel_rename();
            }
            KeyCode::Backspace if control_held => remove_previous_word(&mut window.rename_buffer),
            KeyCode::Backspace => {
                window.rename_buffer.pop();
            }
            _ if !control_held => {
                if let Some(text) = &input.text {
                    window
                        .rename_buffer
                        .extend(text.chars().filter(|character| !character.is_control()));
                }
            }
            _ => {}
        }
    }
}

fn remove_previous_word(text: &mut String) {
    while text.ends_with(char::is_whitespace) {
        text.pop();
    }
    while text
        .chars()
        .last()
        .is_some_and(|character| !character.is_whitespace())
    {
        text.pop();
    }
}

#[cfg(test)]
mod build_menu_input_tests {
    use super::remove_previous_word;

    #[test]
    fn control_backspace_removes_trailing_space_and_previous_word() {
        let mut query = "fast belt   ".to_string();
        remove_previous_word(&mut query);
        assert_eq!(query, "fast ");
    }

    #[test]
    fn unicode_backspace_removes_one_scalar() {
        let mut query = "belt 🏭".to_string();
        query.pop();
        assert_eq!(query, "belt ");
    }
}

#[derive(SystemParam)]
pub(crate) struct FullscreenMapInputResources<'w, 's> {
    keyboard: Option<Res<'w, ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<'w, ButtonInput<MouseButton>>>,
    mouse_motion: Option<Res<'w, AccumulatedMouseMotion>>,
    mouse_scroll: Option<Res<'w, AccumulatedMouseScroll>>,
    sim: Res<'w, SimResource>,
    cache: Res<'w, MapTextureCache>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    ui_buttons: Query<'w, 's, &'static Interaction, With<Button>>,
    state: ResMut<'w, MapViewState>,
}

pub(crate) fn handle_fullscreen_map_input(mut resources: FullscreenMapInputResources) {
    if !resources.state.open {
        return;
    }

    let (player_x, player_y) = resources.sim.read().player().position_tiles();
    let player_center = Vec2::new(player_x, player_y);
    if resources.state.follow_player {
        resources.state.center_tile = player_center;
    }

    if let Some(keyboard) = resources.keyboard.as_deref() {
        if keyboard.just_pressed(KeyCode::KeyF) {
            resources.state.center_tile = player_center;
            resources.state.follow_player = true;
        }
        if keyboard.just_pressed(KeyCode::Digit1) {
            resources.state.selected_layer = MapLayer::Surface;
        }
        if keyboard.just_pressed(KeyCode::Digit2) {
            resources.state.selected_layer = MapLayer::Resources;
        }
        if keyboard.just_pressed(KeyCode::Digit3) {
            resources.state.selected_layer = MapLayer::Entities;
        }
    }

    let Some(map_bounds) = resources.cache.surface().and_then(|cache| cache.bounds) else {
        return;
    };
    let image_size = fullscreen_map_image_size(resources.windows.iter().next());

    if let Some(mouse_scroll) = resources.mouse_scroll.as_deref() {
        let scroll = mouse_scroll.delta.y;
        if scroll != 0.0 {
            let zoom_factor = (scroll * 0.12).exp();
            resources.state.zoom =
                (resources.state.zoom * zoom_factor).clamp(FULL_MAP_MIN_ZOOM, FULL_MAP_MAX_ZOOM);
            resources.state.center_tile = clamp_map_center(
                map_bounds,
                resources.state.center_tile,
                resources.state.zoom,
                image_size,
            );
        }
    }

    let dragging = resources.mouse_buttons.as_deref().is_some_and(|buttons| {
        buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Middle)
    });
    let interacting_with_button = resources
        .ui_buttons
        .iter()
        .any(|interaction| *interaction != Interaction::None);
    let motion = resources
        .mouse_motion
        .as_deref()
        .map(|motion| motion.delta)
        .unwrap_or(Vec2::ZERO);
    if dragging && !interacting_with_button && motion != Vec2::ZERO {
        let crop = fullscreen_crop_bounds(
            map_bounds,
            resources.state.center_tile,
            resources.state.zoom,
            image_size,
        );
        let display_size = fullscreen_map_display_size(image_size, crop);
        let tiles_per_pixel = Vec2::new(
            crop.width as f32 / display_size.x.max(1.0),
            crop.height as f32 / display_size.y.max(1.0),
        );
        resources.state.center_tile.x -= motion.x * tiles_per_pixel.x;
        resources.state.center_tile.y += motion.y * tiles_per_pixel.y;
        resources.state.follow_player = false;
        resources.state.center_tile = clamp_map_center(
            map_bounds,
            resources.state.center_tile,
            resources.state.zoom,
            image_size,
        );
    }
}

pub fn world_input_blocked(input_state: Option<&AppInputState>) -> bool {
    input_state.is_some_and(|state| state.world_blocked)
}

pub fn escape_consumed(input_state: Option<&AppInputState>) -> bool {
    input_state.is_some_and(|state| state.escape_consumed)
}
