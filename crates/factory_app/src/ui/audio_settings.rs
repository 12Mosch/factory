use bevy::prelude::*;

use crate::audio::{AudioSettings, AudioSettingsWindowState, SoundEvent};

#[derive(Component)]
pub struct AudioSettingsRoot {
    snapshot: AudioSettingsSnapshot,
}

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
struct AudioSettingsSnapshot {
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
    window: Res<AudioSettingsWindowState>,
    mut settings: ResMut<AudioSettings>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    if !window.open {
        return;
    }

    for (interaction, button) in &mut buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }

        sounds.write(SoundEvent::UiClick);
        match button.action {
            AudioSettingsAction::ToggleMute => settings.toggle_muted(),
            AudioSettingsAction::VolumeDown => settings.adjust_volume_steps(-1),
            AudioSettingsAction::VolumeUp => settings.adjust_volume_steps(1),
            AudioSettingsAction::Test => {}
        }
    }
}

pub(crate) fn sync_audio_settings_window(
    mut commands: Commands,
    window: Res<AudioSettingsWindowState>,
    settings: Res<AudioSettings>,
    mut roots: Query<(Entity, &mut AudioSettingsRoot, Option<&Children>)>,
) {
    if !window.open {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let snapshot = audio_settings_snapshot(&settings);
    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        spawn_audio_settings_window(&mut commands, snapshot);
        return;
    };
    for (duplicate, _, _) in roots_iter {
        commands.entity(duplicate).despawn();
    }
    if root.snapshot == snapshot {
        return;
    }
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    root.snapshot = snapshot.clone();
    commands
        .entity(root_entity)
        .with_children(|root| spawn_audio_settings_modal(root, &snapshot));
}

fn audio_settings_snapshot(settings: &AudioSettings) -> AudioSettingsSnapshot {
    AudioSettingsSnapshot {
        muted: settings.muted,
        volume_percent: (settings.volume.clamp(0.0, 1.0) * 100.0).round() as u32,
    }
}

fn spawn_audio_settings_window(commands: &mut Commands, snapshot: AudioSettingsSnapshot) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.56)),
            GlobalZIndex(2700),
            AudioSettingsRoot {
                snapshot: snapshot.clone(),
            },
        ))
        .with_children(|root| spawn_audio_settings_modal(root, &snapshot));
}

fn spawn_audio_settings_modal(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &AudioSettingsSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(88.0),
            max_width: Val::Px(420.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            padding: UiRect::all(Val::Px(16.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.030, 0.032, 0.034, 0.98)),
        BorderColor::all(Color::srgba(0.36, 0.39, 0.34, 0.95)),
    ))
    .with_children(|modal| {
        modal.spawn((
            Text::new("Settings"),
            TextFont::from_font_size(20.0),
            TextColor(Color::srgb(0.94, 0.95, 0.90)),
        ));
        modal.spawn((
            Text::new("Audio"),
            TextFont::from_font_size(14.0),
            TextColor(Color::srgb(0.78, 0.80, 0.76)),
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
