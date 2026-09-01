use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::window::PrimaryWindow;

use crate::audio::AudioSettings;
use crate::build::resources::{
    BlueprintLibraryWindowState, BuildMenuState, BuildPlacementState, PlannerState, PlannerTool,
};
use crate::input::bindings::{ActionInput, InputAction};
use crate::input::resources::AppInputState;
use crate::map::resources::{MapDisplaySettings, MapOverlay, MapTextureCache, MapViewState};
use crate::resources::SimResource;
use crate::save_load::{PendingSaveConfirmation, SaveLoadWindowState};
use crate::simulation::AppPauseState;
use crate::ui::accessibility::UiPreferences;
use crate::ui::controls::ControlRebindState;
use crate::ui::map_view::{
    FULL_MAP_MAX_ZOOM, FULL_MAP_MIN_ZOOM, clamp_map_center, fullscreen_crop_bounds,
    fullscreen_map_display_size, fullscreen_map_image_size,
};
use crate::ui::pause_menu::{NewWorldConfirmation, PauseMenuState};
use crate::ui::resources::{
    CraftingWindowState, EquipmentWindowState, OpenContainer, ProductionStatsWindowState,
    TechnologyWindowState,
};
use crate::ui::settings::{SettingsTab, SettingsWindowState};

#[derive(SystemParam)]
pub(crate) struct WorldBlockingWindows<'w, 's> {
    map: Res<'w, MapViewState>,
    stats: Res<'w, ProductionStatsWindowState>,
    crafting: Res<'w, CraftingWindowState>,
    settings: Res<'w, SettingsWindowState>,
    pause: Res<'w, PauseMenuState>,
    save_load: Res<'w, SaveLoadWindowState>,
    build_menu: Res<'w, BuildMenuState>,
    blueprint_library: Res<'w, BlueprintLibraryWindowState>,
    equipment: Res<'w, EquipmentWindowState>,
    app_pause: Res<'w, AppPauseState>,
    input_focus: Option<Res<'w, InputFocus>>,
    editable_texts: Query<'w, 's, Entity, With<EditableText>>,
}

/// Open/closed state of every window that blocks world interaction.
struct WindowOpenFlags {
    map: bool,
    stats: bool,
    crafting: bool,
    settings: bool,
    pause: bool,
    save_load: bool,
    build_menu: bool,
    blueprint_library: bool,
    equipment: bool,
}

impl WorldBlockingWindows<'_, '_> {
    fn any_open(&self) -> bool {
        self.app_pause.is_paused()
            || world_blocking_windows_open(WindowOpenFlags {
                map: self.map.open,
                stats: self.stats.open,
                crafting: self.crafting.open,
                settings: self.settings.open,
                pause: self.pause.open,
                save_load: self.save_load.open,
                build_menu: self.build_menu.open,
                blueprint_library: self.blueprint_library.open,
                equipment: self.equipment.open,
            })
            || self
                .input_focus
                .as_deref()
                .and_then(InputFocus::get)
                .is_some_and(|focused| self.editable_texts.contains(focused))
    }
}

fn world_blocking_windows_open(flags: WindowOpenFlags) -> bool {
    flags.map
        || flags.stats
        || flags.crafting
        || flags.settings
        || flags.pause
        || flags.save_load
        || flags.build_menu
        || flags.blueprint_library
        || flags.equipment
}

pub(crate) fn reset_app_input_state(
    windows: WorldBlockingWindows,
    mut input_state: ResMut<AppInputState>,
) {
    input_state.world_blocked = windows.any_open();
    input_state.escape_consumed = false;
}

#[derive(SystemParam)]
pub(crate) struct PanelInputResources<'w, 's> {
    input_state: ResMut<'w, AppInputState>,
    sim: Res<'w, SimResource>,
    map: ResMut<'w, MapViewState>,
    map_settings: ResMut<'w, MapDisplaySettings>,
    stats: ResMut<'w, ProductionStatsWindowState>,
    crafting: ResMut<'w, CraftingWindowState>,
    settings: ResMut<'w, SettingsWindowState>,
    control_rebind: Res<'w, ControlRebindState>,
    pause: ResMut<'w, PauseMenuState>,
    app_pause: ResMut<'w, AppPauseState>,
    new_world_confirmation: ResMut<'w, NewWorldConfirmation>,
    audio: Res<'w, AudioSettings>,
    ui_preferences: Res<'w, UiPreferences>,
    technology: ResMut<'w, TechnologyWindowState>,
    save_load: ResMut<'w, SaveLoadWindowState>,
    save_confirmation: ResMut<'w, PendingSaveConfirmation>,
    build_menu: ResMut<'w, BuildMenuState>,
    open_container: ResMut<'w, OpenContainer>,
    build_state: ResMut<'w, BuildPlacementState>,
    planner: ResMut<'w, PlannerState>,
    blueprint_library: ResMut<'w, BlueprintLibraryWindowState>,
    equipment: ResMut<'w, EquipmentWindowState>,
    input_focus: Option<Res<'w, InputFocus>>,
    editable_texts: Query<'w, 's, Entity, With<EditableText>>,
}

pub(crate) fn handle_panel_input(actions: ActionInput, mut resources: PanelInputResources) {
    let editable_text_focused = resources
        .input_focus
        .as_deref()
        .and_then(InputFocus::get)
        .is_some_and(|focused| resources.editable_texts.contains(focused));
    let cancel_pressed = if editable_text_focused {
        actions.text_entry_cancelled()
    } else {
        actions.just_pressed(InputAction::CancelPause)
    };
    if editable_text_focused && !cancel_pressed {
        resources.input_state.world_blocked = true;
        return;
    }

    if resources.build_menu.open {
        if cancel_pressed {
            if resources.build_menu.search_query.is_empty() {
                resources.build_menu.close();
            } else {
                resources.build_menu.search_query.clear();
                resources.build_menu.message = None;
            }
            resources.input_state.escape_consumed = true;
        }
        resources.input_state.world_blocked = world_blocking_windows_open(WindowOpenFlags {
            map: resources.map.open,
            stats: resources.stats.open,
            crafting: resources.crafting.open,
            settings: resources.settings.open,
            pause: resources.pause.open,
            save_load: resources.save_load.open,
            build_menu: resources.build_menu.open,
            blueprint_library: resources.blueprint_library.open,
            equipment: resources.equipment.open,
        });
        return;
    }

    if resources.pause.open {
        if cancel_pressed {
            if resources.new_world_confirmation.awaiting_confirmation {
                resources.new_world_confirmation.awaiting_confirmation = false;
            } else {
                resources.pause.open = false;
                resources.app_pause.resume();
            }
            resources.input_state.escape_consumed = true;
        }
        resources.input_state.world_blocked = world_blocking_windows_open(WindowOpenFlags {
            map: resources.map.open,
            stats: resources.stats.open,
            crafting: resources.crafting.open,
            settings: resources.settings.open,
            pause: resources.pause.open,
            save_load: resources.save_load.open,
            build_menu: resources.build_menu.open,
            blueprint_library: resources.blueprint_library.open,
            equipment: resources.equipment.open,
        });
        return;
    }

    if resources.save_load.open {
        if cancel_pressed {
            if *resources.save_confirmation != PendingSaveConfirmation::None {
                *resources.save_confirmation = PendingSaveConfirmation::None;
            } else {
                resources.save_load.open = false;
                resources.pause.open = true;
            }
            resources.input_state.escape_consumed = true;
        }
        resources.input_state.world_blocked = world_blocking_windows_open(WindowOpenFlags {
            map: resources.map.open,
            stats: resources.stats.open,
            crafting: resources.crafting.open,
            settings: resources.settings.open,
            pause: resources.pause.open,
            save_load: resources.save_load.open,
            build_menu: resources.build_menu.open,
            blueprint_library: resources.blueprint_library.open,
            equipment: resources.equipment.open,
        });
        return;
    }

    if resources.settings.open {
        if resources.control_rebind.capturing.is_some() {
            resources.input_state.world_blocked = true;
            return;
        }
        if actions.just_pressed(InputAction::OpenAudioSettings) {
            if resources.settings.active_tab == SettingsTab::Audio {
                close_settings(&mut resources.settings, &mut resources.pause);
            } else {
                resources.settings.active_tab = SettingsTab::Audio;
            }
        } else if actions.just_pressed(InputAction::OpenGameplaySettings) {
            if resources.settings.active_tab == SettingsTab::Gameplay {
                close_settings(&mut resources.settings, &mut resources.pause);
            } else {
                resources.settings.active_tab = SettingsTab::Gameplay;
            }
        } else if cancel_pressed {
            close_settings(&mut resources.settings, &mut resources.pause);
            resources.input_state.escape_consumed = true;
        }
        resources.input_state.world_blocked = world_blocking_windows_open(WindowOpenFlags {
            map: resources.map.open,
            stats: resources.stats.open,
            crafting: resources.crafting.open,
            settings: resources.settings.open,
            pause: resources.pause.open,
            save_load: resources.save_load.open,
            build_menu: resources.build_menu.open,
            blueprint_library: resources.blueprint_library.open,
            equipment: resources.equipment.open,
        });
        return;
    }

    if actions.just_pressed(InputAction::OpenMap) {
        resources.map.open = !resources.map.open;
        if resources.map.open {
            resources.build_state.selected = None;
            resources.open_container.close();
            if resources.map.follow_player {
                let (x, y) = resources.sim.read().player().position_tiles();
                resources.map.center_tile = Vec2::new(x, y);
            }
        }
    }
    if actions.just_pressed(InputAction::OpenProduction) {
        resources.stats.open = !resources.stats.open;
        if resources.stats.open {
            resources.build_state.selected = None;
            resources.open_container.close();
        }
    }
    if actions.just_pressed(InputAction::OpenCrafting) {
        resources.crafting.open = !resources.crafting.open;
        if resources.crafting.open {
            resources.build_state.selected = None;
            resources.open_container.close();
        }
    }
    if actions.just_pressed(InputAction::OpenAudioSettings) {
        let enemy_preset = resources.sim.read().enemy_settings().preset;
        resources.settings.open_tab(
            SettingsTab::Audio,
            &resources.audio,
            enemy_preset,
            &resources.ui_preferences,
            false,
        );
        resources.build_state.selected = None;
        resources.open_container.close();
    }
    if actions.just_pressed(InputAction::OpenGameplaySettings) {
        let enemy_preset = resources.sim.read().enemy_settings().preset;
        resources.settings.open_tab(
            SettingsTab::Gameplay,
            &resources.audio,
            enemy_preset,
            &resources.ui_preferences,
            false,
        );
        resources.build_state.selected = None;
        resources.open_container.close();
    }
    if actions.just_pressed(InputAction::OpenEquipment) {
        resources.equipment.open = !resources.equipment.open;
        resources.equipment.selected_inventory_slot = None;
        resources.equipment.feedback = None;
        if resources.equipment.open {
            resources.build_state.selected = None;
            resources.open_container.close();
        }
    }
    if actions.just_pressed(InputAction::OpenBlueprintLibrary) {
        if resources.blueprint_library.open {
            resources.blueprint_library.close();
        } else {
            resources.blueprint_library.open = true;
            resources.build_state.selected = None;
            resources.open_container.close();
        }
    }
    if actions.just_pressed(InputAction::OpenBuildMenu) {
        resources.build_menu.open_fresh();
        resources.build_state.selected = None;
        resources.open_container.close();
    }
    if actions.just_pressed(InputAction::ToggleDebugReveal) {
        resources.map_settings.debug_reveal_all = !resources.map_settings.debug_reveal_all;
        resources.map_settings.show_chunk_grid = resources.map_settings.debug_reveal_all;
    }

    if cancel_pressed {
        if resources.map.open {
            resources.map.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.stats.open {
            resources.stats.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.crafting.open {
            resources.crafting.open = false;
            resources.input_state.escape_consumed = true;
        } else if resources.settings.open {
            if resources.settings.close() {
                resources.pause.open = true;
            }
            resources.input_state.escape_consumed = true;
        } else if resources.equipment.open {
            resources.equipment.open = false;
            resources.equipment.selected_inventory_slot = None;
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
        } else if resources.open_container.is_open() {
            resources.open_container.close();
            resources.input_state.escape_consumed = true;
        } else if resources.planner.tool != PlannerTool::None {
            resources.planner.set_tool(PlannerTool::None);
            resources.input_state.escape_consumed = true;
        } else if resources.build_state.selected.is_some() {
            resources.build_state.selected = None;
            resources.build_state.last_status = Default::default();
            resources.input_state.escape_consumed = true;
        } else {
            resources.app_pause.pause();
            resources.pause.open = true;
            resources.input_state.escape_consumed = true;
        }
    }

    resources.input_state.world_blocked = resources.app_pause.is_paused()
        || world_blocking_windows_open(WindowOpenFlags {
            map: resources.map.open,
            stats: resources.stats.open,
            crafting: resources.crafting.open,
            settings: resources.settings.open,
            pause: resources.pause.open,
            save_load: resources.save_load.open,
            build_menu: resources.build_menu.open,
            blueprint_library: resources.blueprint_library.open,
            equipment: resources.equipment.open,
        });
}

fn close_settings(settings: &mut SettingsWindowState, pause: &mut PauseMenuState) {
    if settings.close() {
        pause.open = true;
    }
}

#[derive(SystemParam)]
pub(crate) struct FullscreenMapInputResources<'w, 's> {
    actions: ActionInput<'w>,
    mouse_motion: Option<Res<'w, AccumulatedMouseMotion>>,
    mouse_scroll: Option<Res<'w, AccumulatedMouseScroll>>,
    sim: Res<'w, SimResource>,
    cache: Res<'w, MapTextureCache>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    ui_buttons: Query<'w, 's, &'static Interaction, With<Button>>,
    state: ResMut<'w, MapViewState>,
    settings: ResMut<'w, MapDisplaySettings>,
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

    if resources.actions.just_pressed(InputAction::MapCenterPlayer) {
        resources.state.center_tile = player_center;
        resources.state.follow_player = true;
    }
    for (action, overlay) in InputAction::MAP_OVERLAYS.into_iter().zip([
        MapOverlay::Pollution,
        MapOverlay::Resources,
        MapOverlay::PowerNetworks,
        MapOverlay::ProductionProblems,
        MapOverlay::Enemies,
        MapOverlay::ConstructionPlans,
    ]) {
        if resources.actions.just_pressed(action) {
            resources.settings.overlays.toggle(overlay);
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

    let dragging = resources.actions.pressed(InputAction::MapDrag);
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
