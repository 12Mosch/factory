use super::common::{
    first_available_hotbar_slot, hotbar_key_for_slot, set_player_inventory_slot, test_app,
};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use factory_app::audio::AudioSettingsWindowState;
use factory_app::build::resources::{
    BlueprintLibraryWindowState, BuildMenuState, BuildPlacementState,
};
use factory_app::input::resources::AppInputState;
use factory_app::map::resources::{
    MapDisplaySettings, MapOverlay, MapTextureBounds, MapTextureCache, MapTextureLayer,
    MapViewState,
};
use factory_app::resources::SimResource;
use factory_app::save_load::SaveLoadWindowState;
use factory_app::ui::enemy_settings::EnemySettingsWindowState;
use factory_app::ui::equipment_window::EquipmentInventoryButton;
use factory_app::ui::resources::{
    CraftingWindowState, EquipmentWindowState, ProductionStatsWindowState, TechnologyWindowState,
};
use std::time::Duration;

#[test]
fn map_view_state_default_values() {
    let state = MapViewState::default();

    assert!(!state.open);
    assert_eq!(state.center_tile, Vec2::ZERO);
    assert_eq!(state.zoom, 1.0);
    assert!(state.follow_player);
    let overlays = MapDisplaySettings::default().overlays;
    assert!(overlays.is_enabled(MapOverlay::Resources));
    assert!(overlays.is_enabled(MapOverlay::Enemies));
    assert!(overlays.is_enabled(MapOverlay::ConstructionPlans));
    assert!(!overlays.is_enabled(MapOverlay::Pollution));
}

#[test]
fn technology_screen_toggles_with_t() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyT);
    app.update();

    assert!(app.world().resource::<TechnologyWindowState>().open);
}

#[test]
fn t_does_not_toggle_technology_window_while_build_menu_is_open() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    press_key(&mut app, KeyCode::KeyB);
    app.update();
    release_key(&mut app, KeyCode::KeyB);
    press_key(&mut app, KeyCode::KeyT);
    app.update();

    assert!(app.world().resource::<BuildMenuState>().open);
    assert!(!app.world().resource::<TechnologyWindowState>().open);

    release_key(&mut app, KeyCode::KeyT);
    app.world_mut().resource_mut::<TechnologyWindowState>().open = true;
    press_key(&mut app, KeyCode::KeyT);
    app.update();

    assert!(app.world().resource::<TechnologyWindowState>().open);
}

#[test]
fn map_screen_toggles_with_m() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let player_center = {
        let (x, y) = app
            .world()
            .resource::<SimResource>()
            .read()
            .player()
            .position_tiles();
        Vec2::new(x, y)
    };

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    app.update();

    let state = app.world().resource::<MapViewState>();
    assert!(state.open);
    assert_eq!(state.center_tile, player_center);
    assert!(state.follow_player);
}

#[test]
fn open_map_follow_updates_center_to_player() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    press_key(&mut app, KeyCode::KeyM);
    app.update();
    release_key(&mut app, KeyCode::KeyM);
    {
        let mut sim_resource = app.world_mut().resource_mut::<SimResource>();
        let mut sim = sim_resource.write_for_tests();
        sim.move_player_by_tiles(3.0, 2.0);
    }
    let player_center = {
        let (x, y) = app
            .world()
            .resource::<SimResource>()
            .read()
            .player()
            .position_tiles();
        Vec2::new(x, y)
    };

    app.update();

    assert_eq!(
        app.world().resource::<MapViewState>().center_tile,
        player_center
    );
}

#[test]
fn fullscreen_map_drag_pan_changes_center_and_disables_follow() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    seed_map_bounds(&mut app);
    app.update();
    press_key(&mut app, KeyCode::KeyM);
    app.update();
    release_key(&mut app, KeyCode::KeyM);
    let before = app.world().resource::<MapViewState>().center_tile;

    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(40.0, -20.0);
    app.update();

    let state = app.world().resource::<MapViewState>();
    assert_ne!(state.center_tile, before);
    assert!(!state.follow_player);
}

#[test]
fn fullscreen_map_drag_pan_ignores_hovered_ui_button() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    seed_map_bounds(&mut app);
    app.update();
    press_key(&mut app, KeyCode::KeyM);
    app.update();
    release_key(&mut app, KeyCode::KeyM);
    let before = app.world().resource::<MapViewState>().center_tile;

    app.world_mut().spawn((Button, Interaction::Hovered));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(40.0, -20.0);
    app.update();

    let state = app.world().resource::<MapViewState>();
    assert_eq!(state.center_tile, before);
    assert!(state.follow_player);
}

#[test]
fn fullscreen_map_wheel_zoom_updates_and_clamps() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    seed_map_bounds(&mut app);
    app.update();
    press_key(&mut app, KeyCode::KeyM);
    app.update();
    release_key(&mut app, KeyCode::KeyM);

    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::new(0.0, 100.0);
    app.update();
    assert_eq!(app.world().resource::<MapViewState>().zoom, 8.0);

    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::new(0.0, -100.0);
    app.update();
    assert_eq!(app.world().resource::<MapViewState>().zoom, 0.25);
}

#[test]
fn fullscreen_map_f_recenters_and_reenables_follow() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    seed_map_bounds(&mut app);
    app.update();
    {
        let mut state = app.world_mut().resource_mut::<MapViewState>();
        state.open = true;
        state.follow_player = false;
        state.center_tile = Vec2::new(80.0, -70.0);
    }
    let player_center = {
        let (x, y) = app
            .world()
            .resource::<SimResource>()
            .read()
            .player()
            .position_tiles();
        Vec2::new(x, y)
    };

    press_key(&mut app, KeyCode::KeyF);
    app.update();

    let state = app.world().resource::<MapViewState>();
    assert_eq!(state.center_tile, player_center);
    assert!(state.follow_player);
}

#[test]
fn fullscreen_map_digit_keys_toggle_overlays_independently() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    seed_map_bounds(&mut app);
    app.update();
    app.world_mut().resource_mut::<MapViewState>().open = true;

    press_key(&mut app, KeyCode::Digit2);
    app.update();
    assert_overlay_states(&app, [false, false, false, false, true, true]);

    release_key(&mut app, KeyCode::Digit2);
    press_key(&mut app, KeyCode::Digit3);
    app.update();
    assert_overlay_states(&app, [false, false, true, false, true, true]);

    release_key(&mut app, KeyCode::Digit3);
    press_key(&mut app, KeyCode::Digit1);
    app.update();
    assert_overlay_states(&app, [true, false, true, false, true, true]);

    release_key(&mut app, KeyCode::Digit1);
    press_key(&mut app, KeyCode::Digit4);
    app.update();
    assert_overlay_states(&app, [true, false, true, true, true, true]);

    release_key(&mut app, KeyCode::Digit4);
    press_key(&mut app, KeyCode::Digit5);
    app.update();
    assert_overlay_states(&app, [true, false, true, true, false, true]);

    release_key(&mut app, KeyCode::Digit5);
    press_key(&mut app, KeyCode::Digit6);
    app.update();
    assert_overlay_states(&app, [true, false, true, true, false, false]);
    release_key(&mut app, KeyCode::Digit6);
}

fn assert_overlay_states(app: &App, expected: [bool; 6]) {
    let overlays = app.world().resource::<MapDisplaySettings>().overlays;
    for (overlay, expected) in MapOverlay::ALL.into_iter().zip(expected) {
        assert_eq!(overlays.is_enabled(overlay), expected, "{overlay:?}");
    }
}

#[test]
fn world_camera_zoom_is_blocked_while_fullscreen_map_is_open() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    press_key(&mut app, KeyCode::KeyM);
    app.update();
    release_key(&mut app, KeyCode::KeyM);
    let before = camera_scale(&mut app);

    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::new(0.0, 4.0);
    app.update();

    assert_eq!(camera_scale(&mut app), before);
}

#[test]
fn production_stats_toggles_with_p() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyP);
    app.update();

    assert!(app.world().resource::<ProductionStatsWindowState>().open);
}

#[test]
fn crafting_screen_toggles_with_c() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyC);
    app.update();

    assert!(app.world().resource::<CraftingWindowState>().open);

    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.clear_just_pressed(KeyCode::KeyC);
        keyboard.release(KeyCode::KeyC);
    }
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyC);
    app.update();

    assert!(!app.world().resource::<CraftingWindowState>().open);
}

#[test]
fn equipment_window_toggles_with_e_blocks_world_and_closes_with_escape() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    press_key(&mut app, KeyCode::KeyE);
    app.update();
    assert!(app.world().resource::<EquipmentWindowState>().open);
    assert!(app.world().resource::<AppInputState>().world_blocked);

    release_key(&mut app, KeyCode::KeyE);
    press_key(&mut app, KeyCode::Escape);
    app.update();
    assert!(!app.world().resource::<EquipmentWindowState>().open);
    assert!(app.world().resource::<AppInputState>().escape_consumed);
}

#[test]
fn equipment_inventory_armor_click_enqueues_and_applies_equip_command() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let armor = {
        let mut sim_resource = app.world_mut().resource_mut::<SimResource>();
        let mut sim = sim_resource.write_for_tests();
        let armor = factory_data::item_id_by_name(sim.catalog(), "modular_armor");
        set_player_inventory_slot(&mut sim, 10, armor, 1);
        armor
    };
    app.world_mut().resource_mut::<EquipmentWindowState>().open = true;
    app.update();
    app.update();

    {
        let world = app.world_mut();
        let mut buttons = world.query::<(&EquipmentInventoryButton, &mut Interaction)>();
        let (_, mut interaction) = buttons
            .iter_mut(world)
            .find(|(button, _)| button.slot_index == 10)
            .expect("equipment inventory slot should be spawned");
        *interaction = Interaction::Pressed;
    }
    app.update();
    app.update();

    assert_eq!(
        app.world()
            .resource::<SimResource>()
            .read()
            .equipped_armor(),
        Some(armor)
    );
}

#[test]
fn audio_settings_panel_toggles_with_o() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyO);
    app.update();

    assert!(app.world().resource::<AudioSettingsWindowState>().open);

    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.clear_just_pressed(KeyCode::KeyO);
        keyboard.release(KeyCode::KeyO);
    }
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyO);
    app.update();

    assert!(!app.world().resource::<AudioSettingsWindowState>().open);
}

#[test]
fn f3_toggles_map_debug_flags() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F3);
    app.update();

    let settings = app.world().resource::<MapDisplaySettings>();
    assert!(settings.debug_reveal_all);
    assert!(settings.show_chunk_grid);
}

#[test]
fn save_load_window_suppresses_panel_and_debug_hotkeys() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    app.world_mut().resource_mut::<SaveLoadWindowState>().open = true;

    for key in [
        KeyCode::KeyM,
        KeyCode::KeyP,
        KeyCode::KeyC,
        KeyCode::KeyO,
        KeyCode::KeyN,
        KeyCode::KeyB,
        KeyCode::F3,
    ] {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
        app.update();
    }
    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.press(KeyCode::ControlLeft);
        keyboard.press(KeyCode::KeyB);
    }
    app.update();

    assert!(!app.world().resource::<MapViewState>().open);
    assert!(!app.world().resource::<ProductionStatsWindowState>().open);
    assert!(!app.world().resource::<CraftingWindowState>().open);
    assert!(!app.world().resource::<AudioSettingsWindowState>().open);
    assert!(!app.world().resource::<EnemySettingsWindowState>().open);
    assert!(!app.world().resource::<BuildMenuState>().open);
    assert!(!app.world().resource::<BlueprintLibraryWindowState>().open);
    let settings = app.world().resource::<MapDisplaySettings>();
    assert!(!settings.debug_reveal_all);
    assert!(!settings.show_chunk_grid);
}

#[test]
fn open_map_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (slot_index, _) = first_available_hotbar_slot(&app);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyM);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn open_crafting_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (slot_index, _) = first_available_hotbar_slot(&app);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyC);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyC);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn open_settings_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (slot_index, _) = first_available_hotbar_slot(&app);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyO);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyO);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn b_opens_build_menu_and_types_into_search_while_open() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyB);
    app.update();

    assert!(app.world().resource::<BuildMenuState>().open);
    assert!(app.world().resource::<AppInputState>().world_blocked);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(KeyCode::KeyB);
    app.update();
    assert!(app.world().resource::<BuildMenuState>().open);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyB);
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyB,
        logical_key: Key::Character("b".into()),
        state: ButtonState::Pressed,
        text: Some("b".into()),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.update();

    let menu = app.world().resource::<BuildMenuState>();
    assert!(menu.open);
    assert_eq!(menu.search_query, "b");
}

#[test]
fn open_build_menu_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (slot_index, _) = first_available_hotbar_slot(&app);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyB);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(KeyCode::KeyB);
    app.update();
    assert!(app.world().resource::<BuildMenuState>().open);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn escape_closes_build_menu() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyB);
    app.update();
    assert!(app.world().resource::<BuildMenuState>().open);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(KeyCode::KeyB);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();

    assert!(!app.world().resource::<BuildMenuState>().open);
}

#[test]
fn escape_closes_settings_panel() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyO);
    app.update();
    {
        let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keyboard.clear_just_pressed(KeyCode::KeyO);
        keyboard.release(KeyCode::KeyO);
    }
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();

    assert!(!app.world().resource::<AudioSettingsWindowState>().open);
    assert!(app.world().resource::<AppInputState>().escape_consumed);
}

fn seed_map_bounds(app: &mut App) {
    app.world_mut()
        .resource_mut::<MapTextureCache>()
        .layer_mut(MapTextureLayer::Surface)
        .bounds = Some(MapTextureBounds {
        min_x: -128,
        min_y: -128,
        width: 256,
        height: 256,
    });
}

fn press_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

fn release_key(app: &mut App, key: KeyCode) {
    let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keyboard.clear_just_pressed(key);
    keyboard.release(key);
}

fn camera_scale(app: &mut App) -> f32 {
    let mut query = app
        .world_mut()
        .query_filtered::<&Projection, With<Camera2d>>();
    query
        .iter(app.world())
        .find_map(|projection| match projection {
            Projection::Orthographic(orthographic) => Some(orthographic.scale),
            _ => None,
        })
        .expect("test app should spawn an orthographic 2d camera")
}
