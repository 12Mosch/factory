use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_sim::{EnemyDifficultyPreset, SimCommand};

use crate::audio::{AudioSettings, SoundEvent};
use crate::resources::SimResource;
use crate::save_load::SaveLoadWindowState;
use crate::simulation::SimCommandRequest;
use crate::ui::accessibility::{
    AccessibilitySettingsSnapshot, DisplaySettingsSnapshot, UiPreferences,
    spawn_accessibility_settings_content, spawn_display_settings_content,
};
use crate::ui::audio_settings::{
    AudioSettingsSnapshot, audio_settings_snapshot, spawn_audio_settings_content,
};
use crate::ui::enemy_settings::{
    EnemySettingsSnapshot, enemy_settings_snapshot, spawn_enemy_settings_content,
};
use crate::ui::layout::PANEL_SCROLLBAR_WIDTH;
use crate::ui::window_sync::{WindowRootQuery, sync_window};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsTab {
    #[default]
    Gameplay,
    Audio,
    Display,
    Controls,
    Accessibility,
}

impl SettingsTab {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Gameplay, "GAMEPLAY"),
        (Self::Audio, "AUDIO"),
        (Self::Display, "DISPLAY"),
        (Self::Controls, "CONTROLS"),
        (Self::Accessibility, "ACCESSIBILITY"),
    ];
}

/// Values owned by the settings session. Audio and gameplay currently apply
/// immediately for compatibility; Apply acknowledges those changes and is the
/// commit point for future settings that cannot be applied one value at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingSettingsValues {
    pub audio_muted: bool,
    pub audio_volume: f32,
    pub enemy_preset: EnemyDifficultyPreset,
    pub ui_scale_percent: u16,
    pub readable_high_contrast: bool,
}

impl Default for PendingSettingsValues {
    fn default() -> Self {
        let audio = AudioSettings::default();
        Self {
            audio_muted: audio.muted,
            audio_volume: audio.volume,
            enemy_preset: EnemyDifficultyPreset::Standard,
            ui_scale_percent: 100,
            readable_high_contrast: false,
        }
    }
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SettingsWindowState {
    pub open: bool,
    pub active_tab: SettingsTab,
    pub pending_values: PendingSettingsValues,
    pub dirty: bool,
    return_to_pause_menu: bool,
}

impl SettingsWindowState {
    /// Opens a tab with a fresh pending snapshot of every configurable value.
    pub fn open_tab(
        &mut self,
        tab: SettingsTab,
        audio: &AudioSettings,
        enemy_preset: EnemyDifficultyPreset,
        ui_preferences: &UiPreferences,
        return_to_pause_menu: bool,
    ) {
        self.open = true;
        self.active_tab = tab;
        self.pending_values = PendingSettingsValues {
            audio_muted: audio.muted,
            audio_volume: audio.volume,
            enemy_preset,
            ui_scale_percent: ui_preferences.scale_percent,
            readable_high_contrast: ui_preferences.readable_high_contrast,
        };
        self.dirty = false;
        self.return_to_pause_menu = return_to_pause_menu;
    }

    /// Closes settings and reports whether the pause menu should be restored.
    pub fn close(&mut self) -> bool {
        self.open = false;
        self.dirty = false;
        std::mem::take(&mut self.return_to_pause_menu)
    }
}

#[derive(Component)]
pub struct SettingsMenuButton;

/// Marks the settings body that owns wheel and trackpad scrolling.
#[derive(Component)]
pub struct SettingsScrollArea;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsTabButton {
    pub tab: SettingsTab,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsActionButton {
    pub action: SettingsAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsAction {
    Apply,
    Reset,
    Back,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SettingsSnapshot {
    active_tab: SettingsTab,
    dirty: bool,
    audio: AudioSettingsSnapshot,
    gameplay: EnemySettingsSnapshot,
    display: DisplaySettingsSnapshot,
    accessibility: AccessibilitySettingsSnapshot,
}

type MenuButtonQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (Changed<Interaction>, With<Button>, With<SettingsMenuButton>),
>;
type TabButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static SettingsTabButton),
    (Changed<Interaction>, With<Button>),
>;
type ActionButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static SettingsActionButton),
    (Changed<Interaction>, With<Button>),
>;

#[derive(SystemParam)]
pub(crate) struct SettingsButtonQueries<'w, 's> {
    menu: MenuButtonQuery<'w, 's>,
    tabs: TabButtonQuery<'w, 's>,
    actions: ActionButtonQuery<'w, 's>,
}

#[derive(SystemParam)]
pub(crate) struct SettingsButtonResources<'w> {
    window: ResMut<'w, SettingsWindowState>,
    save_load: ResMut<'w, SaveLoadWindowState>,
    audio: ResMut<'w, AudioSettings>,
    sim: Res<'w, SimResource>,
    sim_commands: MessageWriter<'w, SimCommandRequest>,
    sounds: MessageWriter<'w, SoundEvent>,
    ui_preferences: ResMut<'w, UiPreferences>,
}

/// Handles settings entry, tab navigation, applying, resetting, and closing.
pub(crate) fn handle_settings_buttons(
    mut buttons: SettingsButtonQueries,
    mut resources: SettingsButtonResources,
) {
    for interaction in &mut buttons.menu {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let enemy_preset = resources.sim.read().enemy_settings().preset;
        resources.window.open_tab(
            SettingsTab::Gameplay,
            &resources.audio,
            enemy_preset,
            &resources.ui_preferences,
            true,
        );
        resources.save_load.open = false;
        resources.sounds.write(SoundEvent::UiClick);
    }

    if !resources.window.open {
        return;
    }

    for (interaction, button) in &mut buttons.tabs {
        if *interaction == Interaction::Pressed {
            resources.window.active_tab = button.tab;
            resources.sounds.write(SoundEvent::UiClick);
        }
    }

    for (interaction, button) in &mut buttons.actions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        resources.sounds.write(SoundEvent::UiClick);
        match button.action {
            SettingsAction::Apply => {
                resources.audio.muted = resources.window.pending_values.audio_muted;
                resources
                    .audio
                    .set_volume(resources.window.pending_values.audio_volume);
                resources.sim_commands.write(SimCommandRequest(
                    SimCommand::SetEnemyRuntimeSettings(
                        resources
                            .window
                            .pending_values
                            .enemy_preset
                            .config()
                            .runtime,
                    ),
                ));
                resources.window.dirty = false;
                resources
                    .ui_preferences
                    .set_scale_percent(resources.window.pending_values.ui_scale_percent);
                resources.ui_preferences.readable_high_contrast =
                    resources.window.pending_values.readable_high_contrast;
            }
            SettingsAction::Reset => match resources.window.active_tab {
                SettingsTab::Audio => {
                    let defaults = AudioSettings::default();
                    resources.window.pending_values.audio_muted = defaults.muted;
                    resources.window.pending_values.audio_volume = defaults.volume;
                    resources.audio.muted = defaults.muted;
                    resources.audio.set_volume(defaults.volume);
                    resources.window.dirty = true;
                }
                SettingsTab::Gameplay => {
                    resources.window.pending_values.enemy_preset = EnemyDifficultyPreset::Standard;
                    resources.sim_commands.write(SimCommandRequest(
                        SimCommand::SetEnemyRuntimeSettings(
                            EnemyDifficultyPreset::Standard.config().runtime,
                        ),
                    ));
                    resources.window.dirty = true;
                }
                SettingsTab::Display => {
                    resources.window.pending_values.ui_scale_percent = 100;
                    resources.window.dirty = true;
                }
                SettingsTab::Accessibility => {
                    resources.window.pending_values.readable_high_contrast = false;
                    resources.window.dirty = true;
                }
                SettingsTab::Controls => {
                    resources.window.dirty = false;
                }
            },
            SettingsAction::Back => {
                if resources.window.close() {
                    resources.save_load.open = true;
                    resources.save_load.refresh_on_open = true;
                }
            }
        }
    }
}

/// Reconciles the settings modal with the current session snapshot.
pub(crate) fn sync_settings_window(
    mut commands: Commands,
    window: Res<SettingsWindowState>,
    audio: Res<AudioSettings>,
    sim: Res<SimResource>,
    mut roots: WindowRootQuery<SettingsSnapshot>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        window.open,
        true,
        || SettingsSnapshot {
            active_tab: window.active_tab,
            dirty: window.dirty,
            audio: audio_settings_snapshot(&audio),
            gameplay: enemy_settings_snapshot(&sim),
            display: DisplaySettingsSnapshot {
                scale_percent: window.pending_values.ui_scale_percent,
            },
            accessibility: AccessibilitySettingsSnapshot {
                readable_high_contrast: window.pending_values.readable_high_contrast,
            },
        },
        settings_root,
        spawn_settings_window,
    );
}

/// Builds the full-screen modal backdrop that blocks world interaction.
fn settings_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::ZERO,
            right: Val::ZERO,
            top: Val::ZERO,
            bottom: Val::ZERO,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.62)),
        GlobalZIndex(2800),
    )
}

/// Spawns the responsive settings chrome and active-tab contents.
fn spawn_settings_window(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &SettingsSnapshot,
) {
    root.spawn((
        Node {
            width: Val::Vw(92.0),
            max_width: Val::Px(760.0),
            height: Val::Vh(90.0),
            max_height: Val::Vh(90.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            padding: UiRect::all(Val::Px(18.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.030, 0.029, 0.99)),
        BorderColor::all(Color::srgb(0.40, 0.48, 0.36)),
    ))
    .with_children(|modal| {
        modal.spawn((
            Text::new("SETTINGS"),
            TextFont::from_font_size(22.0),
            TextColor(Color::srgb(0.88, 0.94, 0.78)),
        ));
        spawn_tabs(modal, snapshot.active_tab);
        modal
            .spawn((
                Node {
                    flex_grow: 1.0,
                    min_height: Val::ZERO,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(12.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    overflow: Overflow::scroll_y(),
                    scrollbar_width: PANEL_SCROLLBAR_WIDTH,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.045, 0.055, 0.050)),
                BorderColor::all(Color::srgb(0.25, 0.31, 0.23)),
                ScrollArea,
                SettingsScrollArea,
            ))
            .with_children(|content| match snapshot.active_tab {
                SettingsTab::Gameplay => spawn_enemy_settings_content(content, &snapshot.gameplay),
                SettingsTab::Audio => spawn_audio_settings_content(content, &snapshot.audio),
                SettingsTab::Display => spawn_display_settings_content(content, &snapshot.display),
                SettingsTab::Controls => spawn_placeholder(
                    content,
                    "Control settings",
                    "Control rebinding is not available yet. Current shortcuts remain active.",
                ),
                SettingsTab::Accessibility => {
                    spawn_accessibility_settings_content(content, &snapshot.accessibility)
                }
            });
        spawn_actions(modal, snapshot.dirty);
    });
}

/// Spawns the wrapping settings-tab navigation row.
fn spawn_tabs(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, selected: SettingsTab) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(6.0),
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            for (tab, label) in SettingsTab::ALL {
                spawn_button(row, label, SettingsTabButton { tab }, tab == selected);
            }
        });
}

// Follow-up: docs/issues/issue-central-settings-placeholder-pages.md
/// Spawns an explanatory placeholder for settings not implemented yet.
fn spawn_placeholder(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    title: &str,
    explanation: &str,
) {
    parent.spawn((
        Text::new(title),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.94, 0.95, 0.90)),
    ));
    parent.spawn((
        Text::new(explanation),
        TextFont::from_font_size(13.0),
        TextColor(Color::srgb(0.67, 0.71, 0.65)),
    ));
}

/// Spawns fixed, always-reachable Reset, Apply, and Back actions.
fn spawn_actions(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, dirty: bool) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexEnd,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            spawn_button(
                row,
                "Reset",
                SettingsActionButton {
                    action: SettingsAction::Reset,
                },
                false,
            );
            spawn_button(
                row,
                if dirty { "Apply *" } else { "Apply" },
                SettingsActionButton {
                    action: SettingsAction::Apply,
                },
                dirty,
            );
            spawn_button(
                row,
                "Back",
                SettingsActionButton {
                    action: SettingsAction::Back,
                },
                false,
            );
        });
}

/// Spawns a settings button with consistent sizing and selected styling.
fn spawn_button<T: Component>(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    marker: T,
    selected: bool,
) {
    parent
        .spawn((
            Button,
            Node {
                min_width: Val::Px(102.0),
                height: Val::Px(32.0),
                padding: UiRect::horizontal(Val::Px(9.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if selected {
                Color::srgb(0.22, 0.29, 0.20)
            } else {
                Color::srgb(0.08, 0.10, 0.09)
            }),
            BorderColor::all(Color::srgb(0.38, 0.45, 0.34)),
            marker,
        ))
        .with_child((Text::new(label), TextFont::from_font_size(11.0)));
}
