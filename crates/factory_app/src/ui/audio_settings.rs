use bevy::prelude::*;

use crate::audio::{AudioSettings, SoundEvent};
use crate::ui::settings::{SettingsTab, SettingsWindowState};

#[derive(Component)]
pub struct AudioSettingsButton {
    pub action: AudioSettingsAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioSettingsAction {
    ToggleMute,
    VolumeDown,
    VolumeUp,
    Test,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioSettingsSnapshot {
    muted: bool,
    volume_percent: u32,
}

type AudioSettingsButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static AudioSettingsButton),
    (Changed<Interaction>, With<Button>),
>;

pub(crate) fn handle_audio_settings_buttons(
    mut buttons: AudioSettingsButtonQuery,
    mut window: ResMut<SettingsWindowState>,
    mut settings: ResMut<AudioSettings>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !window.open || window.active_tab != SettingsTab::Audio {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let changed = match button.action {
            AudioSettingsAction::ToggleMute => {
                settings.toggle_muted();
                true
            }
            AudioSettingsAction::VolumeDown => {
                settings.adjust_volume_steps(-1);
                true
            }
            AudioSettingsAction::VolumeUp => {
                settings.adjust_volume_steps(1);
                true
            }
            AudioSettingsAction::Test => false,
        };
        if changed {
            window.pending_values.audio_muted = settings.muted;
            window.pending_values.audio_volume = settings.volume;
            window.dirty = true;
            sounds.write(SoundEvent::UiClick);
        } else {
            sounds.write(SoundEvent::AudioTest);
        }
    }
}

pub(crate) fn audio_settings_snapshot(settings: &AudioSettings) -> AudioSettingsSnapshot {
    AudioSettingsSnapshot {
        muted: settings.muted,
        volume_percent: (settings.volume.clamp(0.0, 1.0) * 100.0).round() as u32,
    }
}

pub(crate) fn spawn_audio_settings_content(
    modal: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &AudioSettingsSnapshot,
) {
    modal.spawn((
        Text::new("Master audio"),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.94, 0.95, 0.90)),
    ));
    modal
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(8.0),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|row| {
            spawn_button(
                row,
                if snapshot.muted { "Muted" } else { "Sound On" },
                AudioSettingsAction::ToggleMute,
                118.0,
            );
            spawn_button(row, "-", AudioSettingsAction::VolumeDown, 42.0);
            row.spawn((
                Node {
                    width: Val::Px(62.0),
                    height: Val::Px(32.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_child((
                Text::new(format!("{}%", snapshot.volume_percent)),
                TextFont::from_font_size(13.0),
                TextColor(Color::WHITE),
            ));
            spawn_button(row, "+", AudioSettingsAction::VolumeUp, 42.0);
            spawn_button(row, "Test", AudioSettingsAction::Test, 66.0);
        });
}

fn spawn_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    action: AudioSettingsAction,
    width: f32,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(width),
                height: Val::Px(32.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
            BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.70)),
            AudioSettingsButton { action },
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(13.0),
            TextColor(Color::WHITE),
        ));
}
