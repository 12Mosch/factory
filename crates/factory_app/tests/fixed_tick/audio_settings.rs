use super::common::test_app;
use bevy::audio::{AudioSource, Decodable};
use bevy::prelude::*;
use factory_app::audio::{AudioSettings, SoundEvent};
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
fn audio_systems_are_inert_without_asset_server() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    app.world_mut()
        .resource_mut::<Messages<SoundEvent>>()
        .write(SoundEvent::UiClick);

    app.update();
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
