use super::common::test_app;
use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_app::save_load::{SaveLoadConfig, SaveLoadWindowState};
use factory_app::ui::accessibility::{
    ReadableHighContrastButton, UiPreferences, UiScaleAction, UiScaleButton,
};
use factory_app::ui::enemy_settings::EnemyPresetButton;
use factory_app::ui::settings::{
    SettingsAction, SettingsActionButton, SettingsMenuButton, SettingsScrollArea, SettingsTab,
    SettingsTabButton, SettingsWindowState,
};
use factory_app::{audio::AudioSettings, resources::SimResource};
use factory_sim::EnemyDifficultyPreset;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
fn display_accessibility_and_control_tabs_expose_their_expected_content() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    open_settings_with_key(&mut app, KeyCode::KeyO);

    let world = app.world_mut();
    let mut scroll_areas = world
        .query_filtered::<(&Node, &ScrollPosition), (With<ScrollArea>, With<SettingsScrollArea>)>();
    let (node, _) = scroll_areas
        .single(world)
        .expect("settings content should be an interactive scroll area");
    assert_eq!(node.overflow.y, OverflowAxis::Scroll);

    for (tab, expected_text) in [
        (SettingsTab::Display, "Interface scale"),
        (
            SettingsTab::Controls,
            "Control rebinding is not available yet. Current shortcuts remain active.",
        ),
        (SettingsTab::Accessibility, "Readable high contrast"),
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
fn display_scale_and_readable_contrast_apply_to_global_preferences() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let settings_root = std::env::temp_dir().join(format!("factory-ui-settings-{unique}"));
    let save_config = SaveLoadConfig {
        root_dir: settings_root.clone(),
        ..default()
    };
    app.insert_resource(save_config);
    app.update();
    open_settings_with_key(&mut app, KeyCode::KeyO);

    let display = tab_button(&mut app, SettingsTab::Display);
    press_button(&mut app, display);
    let increase = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &UiScaleButton)>();
        buttons
            .iter(world)
            .find_map(|(entity, button)| (button.0 == UiScaleAction::Increase).then_some(entity))
            .expect("display tab should contain a scale increase button")
    };
    press_button(&mut app, increase);
    assert_eq!(
        app.world()
            .resource::<SettingsWindowState>()
            .pending_values
            .ui_scale_percent,
        125
    );

    let apply = action_button(&mut app, SettingsAction::Apply);
    press_button(&mut app, apply);
    app.update();
    assert_eq!(app.world().resource::<UiPreferences>().scale_percent, 125);
    assert_eq!(app.world().resource::<UiScale>().0, 1.25);

    let accessibility = tab_button(&mut app, SettingsTab::Accessibility);
    press_button(&mut app, accessibility);
    let contrast = single_button::<ReadableHighContrastButton>(&mut app);
    press_button(&mut app, contrast);
    let apply = action_button(&mut app, SettingsAction::Apply);
    press_button(&mut app, apply);
    assert!(
        app.world()
            .resource::<UiPreferences>()
            .readable_high_contrast
    );
    if settings_root.exists() {
        std::fs::remove_dir_all(settings_root).unwrap();
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
