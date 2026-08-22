use super::common::test_app;
use bevy::audio::{AudioSource, Decodable, PlaybackMode, PlaybackSettings};
use bevy::prelude::*;
use factory_app::audio::{AudioAssets, AudioSettings, SoundEvent};
use factory_app::ui::audio_settings::{AudioSettingsAction, AudioSettingsButton};
use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn ui_audio_buttons_emit_click_message() {
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
    let before_muted = app.world().resource::<AudioSettings>().muted;

    let button = app
        .world_mut()
        .query_filtered::<(Entity, &AudioSettingsButton), With<Button>>()
        .iter(app.world())
        .find_map(|(entity, button)| {
            (button.action == AudioSettingsAction::ToggleMute).then_some(entity)
        })
        .expect("audio settings panel should have a mute button");

    *app.world_mut()
        .entity_mut(button)
        .get_mut::<Interaction>()
        .expect("settings button should have interaction") = Interaction::Pressed;
    app.update();

    assert_eq!(app.world().resource::<AudioSettings>().muted, !before_muted);
}

#[test]
fn ui_audio_test_button_emits_one_sample_per_click() {
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

    let button = app
        .world_mut()
        .query_filtered::<(Entity, &AudioSettingsButton), With<Button>>()
        .iter(app.world())
        .find_map(|(entity, button)| (button.action == AudioSettingsAction::Test).then_some(entity))
        .expect("audio settings panel should have a test button");

    assert_eq!(
        press_audio_button(&mut app, button),
        vec![SoundEvent::AudioTest]
    );

    *app.world_mut()
        .entity_mut(button)
        .get_mut::<Interaction>()
        .expect("settings button should have interaction") = Interaction::None;
    app.update();

    assert_eq!(
        press_audio_button(&mut app, button),
        vec![SoundEvent::AudioTest]
    );
}

fn press_audio_button(app: &mut App, button: Entity) -> Vec<SoundEvent> {
    let mut cursor = app
        .world()
        .resource::<Messages<SoundEvent>>()
        .get_cursor_current();

    *app.world_mut()
        .entity_mut(button)
        .get_mut::<Interaction>()
        .expect("settings button should have interaction") = Interaction::Pressed;
    app.update();

    cursor
        .read(app.world().resource::<Messages<SoundEvent>>())
        .copied()
        .collect()
}

#[test]
fn audio_systems_are_inert_without_asset_server() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<Messages<SoundEvent>>()
        .write(SoundEvent::UiClick);

    app.update();
}

#[test]
fn audio_test_sample_respects_master_volume_and_mute() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    app.world_mut().resource_mut::<AudioAssets>().craft_complete = Some(Handle::default());
    {
        let mut settings = app.world_mut().resource_mut::<AudioSettings>();
        settings.muted = false;
        settings.set_volume(0.4);
    }
    app.world_mut().write_message(SoundEvent::AudioTest);

    app.update();

    let playback = app
        .world_mut()
        .query::<&PlaybackSettings>()
        .single(app.world())
        .expect("unmuted audio test should spawn one sample");
    assert!(matches!(playback.mode, PlaybackMode::Despawn));
    assert!((playback.volume.to_linear() - 0.3).abs() < f32::EPSILON);

    let mut muted_app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    muted_app.update();
    muted_app
        .world_mut()
        .resource_mut::<AudioAssets>()
        .craft_complete = Some(Handle::default());
    muted_app.world_mut().resource_mut::<AudioSettings>().muted = true;
    muted_app.world_mut().write_message(SoundEvent::AudioTest);

    muted_app.update();

    assert_eq!(
        muted_app
            .world_mut()
            .query::<&PlaybackSettings>()
            .iter(muted_app.world())
            .count(),
        0
    );
}

#[test]
fn bundled_wav_assets_are_decodable() {
    let audio_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/audio");
    let entries = fs::read_dir(&audio_dir).expect("bundled audio directory should be readable");
    let mut decoded_count = 0;

    for entry in entries {
        let path = entry
            .expect("audio directory entry should be readable")
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("wav") {
            continue;
        }

        let bytes = fs::read(&path).expect("bundled audio asset should be readable");
        let source = AudioSource {
            bytes: bytes.into(),
        };
        let _decoder = source.decoder();
        decoded_count += 1;
    }

    assert!(
        decoded_count > 0,
        "at least one bundled WAV asset should exist"
    );
}
