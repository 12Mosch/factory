use super::common::test_app;
use bevy::prelude::*;
use factory_app::save_load::SaveLoadWindowState;
use factory_app::ui::enemy_settings::EnemyPresetButton;
use factory_app::ui::settings::{
    SettingsAction, SettingsActionButton, SettingsMenuButton, SettingsTab, SettingsTabButton,
    SettingsWindowState,
};
use factory_app::{audio::AudioSettings, resources::SimResource};
use factory_sim::EnemyDifficultyPreset;
use std::time::Duration;

#[test]
fn pause_menu_settings_button_opens_settings_and_back_returns_to_pause() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    app.world_mut().resource_mut::<SaveLoadWindowState>().open = true;
    app.update();

    let settings_button = single_button::<SettingsMenuButton>(&mut app);
    press_button(&mut app, settings_button);

    let settings = app.world().resource::<SettingsWindowState>();
    assert!(settings.open);
    assert_eq!(settings.active_tab, SettingsTab::Gameplay);
    assert!(!app.world().resource::<SaveLoadWindowState>().open);

    let back = action_button(&mut app, SettingsAction::Back);
    press_button(&mut app, back);

    assert!(!app.world().resource::<SettingsWindowState>().open);
    assert!(app.world().resource::<SaveLoadWindowState>().open);
}

#[test]
fn active_tab_shortcuts_return_pause_origin_to_pause_menu() {
    for (tab, key) in [
        (SettingsTab::Gameplay, KeyCode::KeyN),
        (SettingsTab::Audio, KeyCode::KeyO),
    ] {
        let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
        app.update();
        open_settings_from_pause(&mut app);

        if tab != SettingsTab::Gameplay {
            let button = tab_button(&mut app, tab);
            press_button(&mut app, button);
        }
        assert_eq!(
            app.world().resource::<SettingsWindowState>().active_tab,
            tab
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();

        assert!(!app.world().resource::<SettingsWindowState>().open);
        assert!(app.world().resource::<SaveLoadWindowState>().open);
    }
}

#[test]
fn every_placeholder_tab_is_reachable_and_clearly_unavailable() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    open_settings_with_key(&mut app, KeyCode::KeyO);

    for (tab, expected_text) in [
        (
            SettingsTab::Display,
            "Display settings are not available yet.",
        ),
        (
            SettingsTab::Controls,
            "Control rebinding is not available yet. Current shortcuts remain active.",
        ),
        (
            SettingsTab::Accessibility,
            "Accessibility settings are not available yet.",
        ),
    ] {
        let button = tab_button(&mut app, tab);
        press_button(&mut app, button);

        assert_eq!(
            app.world().resource::<SettingsWindowState>().active_tab,
            tab
        );
        assert!(all_text(&mut app).iter().any(|text| text == expected_text));
    }
}

#[test]
fn escape_closes_direct_settings_without_opening_pause_menu() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    open_settings_with_key(&mut app, KeyCode::KeyN);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();

    assert!(!app.world().resource::<SettingsWindowState>().open);
    assert!(!app.world().resource::<SaveLoadWindowState>().open);
}

#[test]
fn apply_acknowledges_immediate_audio_changes_and_reset_restores_defaults() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    open_settings_with_key(&mut app, KeyCode::KeyO);

    let mute = {
        let world = app.world_mut();
        let mut buttons = world.query::<(
            Entity,
            &factory_app::ui::audio_settings::AudioSettingsButton,
        )>();
        buttons
            .iter(world)
            .find_map(|(entity, button)| {
                (button.action == factory_app::ui::audio_settings::AudioSettingsAction::ToggleMute)
                    .then_some(entity)
            })
            .expect("audio tab should contain a mute button")
    };
    press_button(&mut app, mute);
    assert!(app.world().resource::<SettingsWindowState>().dirty);

    let apply = action_button(&mut app, SettingsAction::Apply);
    press_button(&mut app, apply);
    assert!(!app.world().resource::<SettingsWindowState>().dirty);

    let reset = action_button(&mut app, SettingsAction::Reset);
    press_button(&mut app, reset);
    let defaults = AudioSettings::default();
    let audio = app.world().resource::<AudioSettings>();
    assert_eq!(audio.muted, defaults.muted);
    assert_eq!(audio.volume, defaults.volume);
}

#[test]
fn gameplay_tab_keeps_enemy_preset_actions() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    open_settings_with_key(&mut app, KeyCode::KeyN);

    let aggressive = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &EnemyPresetButton)>();
        buttons
            .iter(world)
            .find_map(|(entity, button)| {
                (button.preset == EnemyDifficultyPreset::Aggressive).then_some(entity)
            })
            .expect("gameplay tab should contain an aggressive preset button")
    };
    press_button(&mut app, aggressive);
    app.update();

    let settings = app.world().resource::<SettingsWindowState>();
    assert_eq!(
        settings.pending_values.enemy_preset,
        EnemyDifficultyPreset::Aggressive
    );
    assert!(settings.dirty);
    assert_eq!(
        app.world()
            .resource::<SimResource>()
            .read()
            .enemy_settings()
            .runtime,
        EnemyDifficultyPreset::Aggressive.config().runtime
    );
}

fn open_settings_with_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keyboard.clear_just_pressed(key);
    keyboard.release(key);
}

fn open_settings_from_pause(app: &mut App) {
    app.world_mut().resource_mut::<SaveLoadWindowState>().open = true;
    app.update();
    let settings_button = single_button::<SettingsMenuButton>(app);
    press_button(app, settings_button);
}

fn single_button<T: Component>(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut query = world.query_filtered::<Entity, (With<Button>, With<T>)>();
    query
        .single(world)
        .expect("expected exactly one matching button")
}

fn tab_button(app: &mut App, tab: SettingsTab) -> Entity {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &SettingsTabButton)>();
    query
        .iter(world)
        .find_map(|(entity, button)| (button.tab == tab).then_some(entity))
        .expect("settings tab button should exist")
}

fn action_button(app: &mut App, action: SettingsAction) -> Entity {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &SettingsActionButton)>();
    query
        .iter(world)
        .find_map(|(entity, button)| (button.action == action).then_some(entity))
        .expect("settings action button should exist")
}

fn press_button(app: &mut App, entity: Entity) {
    *app.world_mut()
        .entity_mut(entity)
        .get_mut::<Interaction>()
        .expect("button should have interaction") = Interaction::Pressed;
    app.update();
}

fn all_text(app: &mut App) -> Vec<String> {
    let world = app.world_mut();
    let mut texts = world.query::<&Text>();
    texts.iter(world).map(|text| text.0.clone()).collect()
}
