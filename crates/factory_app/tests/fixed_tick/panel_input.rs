use super::common::{hotbar_key_for_slot, test_app};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use factory_app::audio::AudioSettingsWindowState;
use factory_app::placement::build::buildable_prototypes;
use factory_app::resources::{
    AppInputState, BuildPlacementState, CraftingWindowState, MapDisplaySettings, MapLayer,
    MapTextureBounds, MapTextureCache, MapViewState, ProductionStatsWindowState, SimResource,
    TechnologyWindowState,
};
use std::time::Duration;

#[test]
fn map_view_state_default_values() {
    let state = MapViewState::default();

    assert!(!state.open);
    assert_eq!(state.center_tile, Vec2::ZERO);
    assert_eq!(state.zoom, 1.0);
    assert!(state.follow_player);
    assert_eq!(state.selected_layer, MapLayer::Surface);
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
fn map_screen_toggles_with_m() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let player_center = {
        let (x, y) = app
            .world()
            .resource::<SimResource>()
            .sim
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
        let mut sim = app.world_mut().resource_mut::<SimResource>();
        sim.sim.move_player_by_tiles(3.0, 2.0);
    }
    let player_center = {
        let (x, y) = app
            .world()
            .resource::<SimResource>()
            .sim
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
            .sim
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
fn fullscreen_map_digit_keys_select_layers() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    seed_map_bounds(&mut app);
    app.update();
    app.world_mut().resource_mut::<MapViewState>().open = true;

    press_key(&mut app, KeyCode::Digit2);
    app.update();
    assert_eq!(
        app.world().resource::<MapViewState>().selected_layer,
        MapLayer::Resources
    );

    release_key(&mut app, KeyCode::Digit2);
    press_key(&mut app, KeyCode::Digit3);
    app.update();
    assert_eq!(
        app.world().resource::<MapViewState>().selected_layer,
        MapLayer::Entities
    );

    release_key(&mut app, KeyCode::Digit3);
    press_key(&mut app, KeyCode::Digit1);
    app.update();
    assert_eq!(
        app.world().resource::<MapViewState>().selected_layer,
        MapLayer::Surface
    );
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
fn open_map_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

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
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn open_crafting_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

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
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn open_settings_suppresses_build_hotbar_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

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
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
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
        .layers
        .entry(MapLayer::Surface)
        .or_default()
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
