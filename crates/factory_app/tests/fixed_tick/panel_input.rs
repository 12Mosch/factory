use super::common::{hotbar_key_for_slot, test_app};
use bevy::prelude::*;
use factory_app::audio::AudioSettingsWindowState;
use factory_app::placement::build::buildable_prototypes;
use factory_app::resources::{
    AppInputState, BuildPlacementState, CraftingWindowState, MapDisplaySettings, MapViewState,
    ProductionStatsWindowState, SimResource, TechnologyWindowState,
};
use std::time::Duration;

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

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    app.update();

    assert!(app.world().resource::<MapViewState>().open);
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
