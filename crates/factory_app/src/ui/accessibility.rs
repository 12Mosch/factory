use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy::sprite::Text2dShadow;
use bevy::window::PrimaryWindow;
use serde::{Deserialize, Serialize};

use crate::save_load::SaveLoadConfig;
use crate::ui::settings::{SettingsTab, SettingsWindowState};

pub const MIN_UI_SCALE_PERCENT: u16 = 75;
pub const MAX_UI_SCALE_PERCENT: u16 = 200;
const UI_SCALE_STEP_PERCENT: u16 = 25;
const MIN_LOGICAL_VIEWPORT_WIDTH: f32 = 800.0;
const MIN_LOGICAL_VIEWPORT_HEIGHT: f32 = 450.0;
const PERSISTENCE_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct UiPreferences {
    pub scale_percent: u16,
    pub readable_high_contrast: bool,
    settings_path: PathBuf,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            scale_percent: 100,
            readable_high_contrast: false,
            settings_path: PathBuf::new(),
        }
    }
}

impl UiPreferences {
    /// Returns the configured percentage as Bevy's multiplicative scale.
    pub fn requested_scale(&self) -> f32 {
        f32::from(self.scale_percent) / 100.0
    }

    /// Stores a scale percentage after constraining it to the supported range.
    pub fn set_scale_percent(&mut self, percent: u16) {
        self.scale_percent = percent.clamp(MIN_UI_SCALE_PERCENT, MAX_UI_SCALE_PERCENT);
    }
}

#[derive(Resource, Default)]
pub struct UiPreferencesPersistenceState {
    last_saved: Option<UiPreferencesFile>,
    retry_after: Option<Duration>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct UiPreferencesFile {
    pub scale_percent: u16,
    pub readable_high_contrast: bool,
}

impl Default for UiPreferencesFile {
    fn default() -> Self {
        Self {
            scale_percent: 100,
            readable_high_contrast: false,
        }
    }
}

impl UiPreferencesFile {
    /// Builds the stable on-disk representation of the current preferences.
    fn from_preferences(preferences: &UiPreferences) -> Self {
        Self {
            scale_percent: preferences
                .scale_percent
                .clamp(MIN_UI_SCALE_PERCENT, MAX_UI_SCALE_PERCENT),
            readable_high_contrast: preferences.readable_high_contrast,
        }
    }

    /// Sanitizes values loaded from files created by older or edited versions.
    fn normalize(&mut self) {
        self.scale_percent = self
            .scale_percent
            .clamp(MIN_UI_SCALE_PERCENT, MAX_UI_SCALE_PERCENT);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ReadableWorldLabel {
    base_font_size: f32,
}

impl ReadableWorldLabel {
    /// Records the unscaled font size used by the world-label policy.
    pub const fn new(base_font_size: f32) -> Self {
        Self { base_font_size }
    }
}

#[derive(Component, Clone, Copy)]
pub(crate) struct NormalTextColor(Color);

#[derive(Component, Clone, Copy)]
pub(crate) struct NormalBackgroundColor(Color);

#[derive(Component, Clone, Copy)]
pub(crate) struct NormalBorderColor(BorderColor);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiScaleButton(pub UiScaleAction);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiScaleAction {
    Decrease,
    Increase,
}

#[derive(Component)]
pub struct ReadableHighContrastButton;

type UiScaleButtonQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static UiScaleButton),
    (Changed<Interaction>, With<Button>),
>;
type ContrastButtonQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<ReadableHighContrastButton>,
    ),
>;
type ChangedTextColorQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut TextColor,
        Option<&'static NormalTextColor>,
    ),
    (With<Text>, Or<(Added<TextColor>, Changed<TextColor>)>),
>;
type ChangedBackgroundColorQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut BackgroundColor,
        Option<&'static NormalBackgroundColor>,
    ),
    Or<(Added<BackgroundColor>, Changed<BackgroundColor>)>,
>;
type ChangedBorderColorQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut BorderColor,
        Option<&'static NormalBorderColor>,
    ),
    Or<(Added<BorderColor>, Changed<BorderColor>)>,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DisplaySettingsSnapshot {
    pub scale_percent: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AccessibilitySettingsSnapshot {
    pub readable_high_contrast: bool,
}

/// Applies Display and Accessibility button presses to the pending session.
pub(crate) fn handle_accessibility_settings_buttons(
    mut scale_buttons: UiScaleButtonQuery,
    mut contrast_buttons: ContrastButtonQuery,
    mut window: ResMut<SettingsWindowState>,
) {
    if !window.open {
        return;
    }

    if window.active_tab == SettingsTab::Display {
        for (interaction, button) in &mut scale_buttons {
            if *interaction != Interaction::Pressed {
                continue;
            }
            let current = window.pending_values.ui_scale_percent;
            window.pending_values.ui_scale_percent = match button.0 {
                UiScaleAction::Decrease => current.saturating_sub(UI_SCALE_STEP_PERCENT),
                UiScaleAction::Increase => current.saturating_add(UI_SCALE_STEP_PERCENT),
            }
            .clamp(MIN_UI_SCALE_PERCENT, MAX_UI_SCALE_PERCENT);
            window.dirty = true;
        }
    }

    if window.active_tab == SettingsTab::Accessibility {
        for interaction in &mut contrast_buttons {
            if *interaction == Interaction::Pressed {
                window.pending_values.readable_high_contrast =
                    !window.pending_values.readable_high_contrast;
                window.dirty = true;
            }
        }
    }
}

/// Spawns the interface-scale controls for the Display tab.
pub(crate) fn spawn_display_settings_content(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &DisplaySettingsSnapshot,
) {
    spawn_heading(parent, "Interface scale");
    parent.spawn((
        Text::new("Scales panels, text, spacing, and mouse targets together."),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.72, 0.76, 0.69)),
    ));
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|row| {
            spawn_control_button(row, "−", Some(UiScaleAction::Decrease), false);
            row.spawn((
                Node {
                    width: Val::Px(96.0),
                    height: Val::Px(38.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.03, 0.04, 0.035)),
                BorderColor::all(Color::srgb(0.43, 0.53, 0.38)),
            ))
            .with_child((
                Text::new(format!("{}%", snapshot.scale_percent)),
                TextFont::from_font_size(15.0),
                TextColor(Color::srgb(0.92, 0.96, 0.84)),
            ));
            spawn_control_button(row, "+", Some(UiScaleAction::Increase), false);
        });
    parent.spawn((
        Text::new(
            "On compact windows, scaling is responsively limited to keep the working area visible.",
        ),
        TextFont::from_font_size(11.0),
        TextColor(Color::srgb(0.62, 0.68, 0.59)),
    ));
}

/// Spawns the readable high-contrast control for the Accessibility tab.
pub(crate) fn spawn_accessibility_settings_content(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &AccessibilitySettingsSnapshot,
) {
    spawn_heading(parent, "Readable high contrast");
    parent.spawn((
        Text::new("Raises text and control contrast and enlarges essential labels in the world."),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.72, 0.76, 0.69)),
    ));
    spawn_control_button(
        parent,
        if snapshot.readable_high_contrast {
            "ON"
        } else {
            "OFF"
        },
        None,
        snapshot.readable_high_contrast,
    );
}

/// Spawns a settings-section heading using the shared visual treatment.
fn spawn_heading(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, text: &str) {
    parent.spawn((
        Text::new(text),
        TextFont::from_font_size(16.0),
        TextColor(Color::srgb(0.94, 0.95, 0.90)),
    ));
}

/// Spawns a minimum-size accessibility control and its interaction marker.
fn spawn_control_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    scale_action: Option<UiScaleAction>,
    selected: bool,
) {
    let mut button = parent.spawn((
        Button,
        Node {
            min_width: Val::Px(52.0),
            min_height: Val::Px(38.0),
            padding: UiRect::horizontal(Val::Px(12.0)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(if selected {
            Color::srgb(0.24, 0.34, 0.18)
        } else {
            Color::srgb(0.07, 0.09, 0.075)
        }),
        BorderColor::all(if selected {
            Color::srgb(0.82, 0.94, 0.40)
        } else {
            Color::srgb(0.43, 0.53, 0.38)
        }),
    ));
    if let Some(action) = scale_action {
        button.insert(UiScaleButton(action));
    } else {
        button.insert(ReadableHighContrastButton);
    }
    button.with_child((
        Text::new(label),
        TextFont::from_font_size(14.0),
        TextColor(Color::WHITE),
    ));
}

/// Loads preferences once at startup and establishes the persistence baseline.
pub(crate) fn load_persisted_ui_preferences(
    config: Res<SaveLoadConfig>,
    mut preferences: ResMut<UiPreferences>,
    mut persistence: ResMut<UiPreferencesPersistenceState>,
) {
    let path = ui_preferences_path(&config);
    let file = read_ui_preferences_file(&path).unwrap_or_default();
    preferences.settings_path = path;
    preferences.set_scale_percent(file.scale_percent);
    preferences.readable_high_contrast = file.readable_high_contrast;
    persistence.last_saved = Some(UiPreferencesFile::from_preferences(&preferences));
}

/// Persists changed preferences and retries transient failures with backoff.
pub(crate) fn save_ui_preferences_if_changed(
    time: Res<Time<Real>>,
    preferences: Res<UiPreferences>,
    mut persistence: ResMut<UiPreferencesPersistenceState>,
) {
    if preferences.settings_path.as_os_str().is_empty() {
        return;
    }
    let file = UiPreferencesFile::from_preferences(&preferences);
    if persistence.last_saved.as_ref() == Some(&file) {
        persistence.retry_after = None;
        return;
    }

    let now = time.elapsed();
    if !preferences.is_changed()
        && persistence
            .retry_after
            .is_none_or(|retry_after| now < retry_after)
    {
        return;
    }
    if write_ui_preferences_file(&preferences.settings_path, &file).is_ok() {
        persistence.last_saved = Some(file);
        persistence.retry_after = None;
    } else {
        persistence.retry_after = Some(now + PERSISTENCE_RETRY_DELAY);
    }
}

/// Reads and normalizes a UI preference file, returning `None` when invalid.
pub fn read_ui_preferences_file(path: &Path) -> Option<UiPreferencesFile> {
    let text = fs::read_to_string(path).ok()?;
    let mut file = ron::from_str::<UiPreferencesFile>(&text).ok()?;
    file.normalize();
    Some(file)
}

/// Creates parent directories and writes a UI preference file as readable RON.
pub fn write_ui_preferences_file(
    path: &Path,
    file: &UiPreferencesFile,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = ron::ser::to_string_pretty(file, ron::ser::PrettyConfig::default())
        .unwrap_or_else(|_| "(scale_percent:100,readable_high_contrast:false)".to_string());
    fs::write(path, text)
}

/// Returns the UI preference path within the configured save directory.
pub fn ui_preferences_path(config: &SaveLoadConfig) -> PathBuf {
    config.root_dir.join("ui-settings.ron")
}

/// Synchronizes Bevy's global UI scale with the responsive effective scale.
pub(crate) fn sync_ui_scale(
    preferences: Res<UiPreferences>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
) {
    let requested = preferences.requested_scale();
    let effective = windows.single().map_or(requested, |window| {
        effective_ui_scale(
            requested,
            window.resolution.width(),
            window.resolution.height(),
        )
    });
    if (ui_scale.0 - effective).abs() > f32::EPSILON {
        ui_scale.0 = effective;
    }
}

/// Limits a requested scale so the viewport retains a usable logical area.
pub fn effective_ui_scale(requested: f32, viewport_width: f32, viewport_height: f32) -> f32 {
    let requested = requested.clamp(
        f32::from(MIN_UI_SCALE_PERCENT) / 100.0,
        f32::from(MAX_UI_SCALE_PERCENT) / 100.0,
    );
    let responsive_limit = (viewport_width / MIN_LOGICAL_VIEWPORT_WIDTH)
        .min(viewport_height / MIN_LOGICAL_VIEWPORT_HEIGHT)
        .max(f32::from(MIN_UI_SCALE_PERCENT) / 100.0);
    requested.min(responsive_limit)
}

/// Applies or restores the complete UI palette when the preset changes.
pub(crate) fn refresh_high_contrast_palette(
    mut commands: Commands,
    preferences: Res<UiPreferences>,
    mut text_colors: Query<(Entity, &mut TextColor, Option<&NormalTextColor>), With<Text>>,
    mut backgrounds: Query<(Entity, &mut BackgroundColor, Option<&NormalBackgroundColor>)>,
    mut borders: Query<(Entity, &mut BorderColor, Option<&NormalBorderColor>)>,
) {
    if !preferences.is_changed() {
        return;
    }

    if preferences.readable_high_contrast {
        for (entity, mut color, normal) in &mut text_colors {
            apply_text_contrast(&mut commands, entity, &mut color, normal);
        }
        for (entity, mut color, normal) in &mut backgrounds {
            apply_background_contrast(&mut commands, entity, &mut color, normal);
        }
        for (entity, mut color, normal) in &mut borders {
            apply_border_contrast(&mut commands, entity, &mut color, normal);
        }
    } else {
        for (entity, mut color, normal) in &mut text_colors {
            if let Some(normal) = normal {
                color.0 = normal.0;
                commands.entity(entity).remove::<NormalTextColor>();
            }
        }
        for (entity, mut color, normal) in &mut backgrounds {
            if let Some(normal) = normal {
                color.0 = normal.0;
                commands.entity(entity).remove::<NormalBackgroundColor>();
            }
        }
        for (entity, mut color, normal) in &mut borders {
            if let Some(normal) = normal {
                *color = normal.0;
                commands.entity(entity).remove::<NormalBorderColor>();
            }
        }
    }
}

/// Applies high contrast to newly spawned or subsequently recolored UI nodes.
pub(crate) fn update_high_contrast_palette(
    mut commands: Commands,
    preferences: Res<UiPreferences>,
    mut text_colors: ChangedTextColorQuery,
    mut backgrounds: ChangedBackgroundColorQuery,
    mut borders: ChangedBorderColorQuery,
) {
    if !preferences.readable_high_contrast {
        return;
    }
    for (entity, mut color, normal) in &mut text_colors {
        apply_text_contrast(&mut commands, entity, &mut color, normal);
    }
    for (entity, mut color, normal) in &mut backgrounds {
        apply_background_contrast(&mut commands, entity, &mut color, normal);
    }
    for (entity, mut color, normal) in &mut borders {
        apply_border_contrast(&mut commands, entity, &mut color, normal);
    }
}

/// Stores a text color's normal value before applying its contrast mapping.
fn apply_text_contrast(
    commands: &mut Commands,
    entity: Entity,
    color: &mut TextColor,
    stored: Option<&NormalTextColor>,
) {
    let normal = stored.map_or(color.0, |normal| normal.0);
    if color.0 == high_contrast_text(normal) {
        return;
    }
    let normal = if stored.is_some() { color.0 } else { normal };
    commands.entity(entity).insert(NormalTextColor(normal));
    color.0 = high_contrast_text(normal);
}

/// Stores a background's normal value before applying its contrast mapping.
fn apply_background_contrast(
    commands: &mut Commands,
    entity: Entity,
    color: &mut BackgroundColor,
    stored: Option<&NormalBackgroundColor>,
) {
    let normal = stored.map_or(color.0, |normal| normal.0);
    if color.0 == high_contrast_background(normal) {
        return;
    }
    let normal = if stored.is_some() { color.0 } else { normal };
    commands
        .entity(entity)
        .insert(NormalBackgroundColor(normal));
    color.0 = high_contrast_background(normal);
}

/// Stores all normal border sides before applying their contrast mappings.
fn apply_border_contrast(
    commands: &mut Commands,
    entity: Entity,
    color: &mut BorderColor,
    stored: Option<&NormalBorderColor>,
) {
    let normal = stored.map_or(*color, |normal| normal.0);
    if *color == high_contrast_borders(normal) {
        return;
    }
    let normal = if stored.is_some() { *color } else { normal };
    commands.entity(entity).insert(NormalBorderColor(normal));
    *color = high_contrast_borders(normal);
}

/// Maps text to white, warning yellow, or success green with strong opacity.
fn high_contrast_text(color: Color) -> Color {
    let source = color.to_srgba();
    let max = source.red.max(source.green).max(source.blue);
    let min = source.red.min(source.green).min(source.blue);
    let emphasized = max - min > 0.18;
    if emphasized && source.red > source.blue && source.green < source.red * 0.85 {
        Color::srgba(1.0, 0.86, 0.28, source.alpha.max(0.96))
    } else if emphasized && source.green > source.red {
        Color::srgba(0.72, 1.0, 0.58, source.alpha.max(0.96))
    } else {
        Color::srgba(1.0, 1.0, 1.0, source.alpha.max(0.96))
    }
}

/// Maps opaque backgrounds onto two separated near-black luminance levels.
fn high_contrast_background(color: Color) -> Color {
    let source = color.to_srgba();
    if source.alpha <= f32::EPSILON {
        return Color::NONE;
    }
    let luminance = 0.2126 * source.red + 0.7152 * source.green + 0.0722 * source.blue;
    let level = if luminance > 0.16 { 0.14 } else { 0.015 };
    Color::srgba(level, level, level, source.alpha)
}

/// Maps a visible border to the preset's bright interaction outline.
fn high_contrast_border(color: Color) -> Color {
    let source = color.to_srgba();
    if source.alpha <= f32::EPSILON {
        return Color::NONE;
    }
    Color::srgba(0.92, 0.98, 0.62, source.alpha.max(0.88))
}

/// Maps each border side independently so asymmetric borders remain reversible.
fn high_contrast_borders(colors: BorderColor) -> BorderColor {
    BorderColor {
        top: high_contrast_border(colors.top),
        right: high_contrast_border(colors.right),
        bottom: high_contrast_border(colors.bottom),
        left: high_contrast_border(colors.left),
    }
}

/// Restyles every accessible world label when preferences change.
pub(crate) fn refresh_world_label_readability(
    preferences: Res<UiPreferences>,
    mut labels: Query<(&ReadableWorldLabel, &mut TextFont, &mut Text2dShadow)>,
) {
    if !preferences.is_changed() {
        return;
    }
    for (label, mut font, mut shadow) in &mut labels {
        apply_world_label_style(&preferences, label, &mut font, &mut shadow);
    }
}

/// Styles newly spawned world labels without scanning unchanged labels.
pub(crate) fn style_new_world_labels(
    preferences: Res<UiPreferences>,
    mut labels: Query<
        (&ReadableWorldLabel, &mut TextFont, &mut Text2dShadow),
        Added<ReadableWorldLabel>,
    >,
) {
    for (label, mut font, mut shadow) in &mut labels {
        apply_world_label_style(&preferences, label, &mut font, &mut shadow);
    }
}

/// Applies the requested scale, readable floor, and shadow to one world label.
fn apply_world_label_style(
    preferences: &UiPreferences,
    label: &ReadableWorldLabel,
    font: &mut TextFont,
    shadow: &mut Text2dShadow,
) {
    let readable_floor = if preferences.readable_high_contrast {
        1.5
    } else {
        1.0
    };
    let scale = preferences.requested_scale().max(readable_floor);
    font.font_size = FontSize::Px(label.base_font_size * scale);
    *shadow = if preferences.readable_high_contrast {
        Text2dShadow {
            offset: Vec2::new(2.0, -2.0),
            color: Color::BLACK,
        }
    } else {
        Text2dShadow::default()
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn requested_scale_is_clamped_to_supported_range() {
        let mut preferences = UiPreferences::default();
        preferences.set_scale_percent(10);
        assert_eq!(preferences.scale_percent, MIN_UI_SCALE_PERCENT);
        preferences.set_scale_percent(999);
        assert_eq!(preferences.scale_percent, MAX_UI_SCALE_PERCENT);
    }

    #[test]
    fn representative_resolutions_keep_a_usable_logical_viewport() {
        assert_eq!(effective_ui_scale(2.0, 1_280.0, 720.0), 1.6);
        assert_eq!(effective_ui_scale(2.0, 1_920.0, 1_080.0), 2.0);
        assert_eq!(effective_ui_scale(0.75, 1_280.0, 720.0), 0.75);
        assert_eq!(effective_ui_scale(1.25, 1_920.0, 1_080.0), 1.25);
    }

    #[test]
    fn preferences_round_trip_and_legacy_defaults_are_supported() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("factory-ui-preferences-{unique}"));
        let path = root.join("ui-settings.ron");
        let file = UiPreferencesFile {
            scale_percent: 175,
            readable_high_contrast: true,
        };
        write_ui_preferences_file(&path, &file).unwrap();
        assert_eq!(read_ui_preferences_file(&path), Some(file));

        fs::write(&path, "(scale_percent:125)").unwrap();
        assert_eq!(
            read_ui_preferences_file(&path),
            Some(UiPreferencesFile {
                scale_percent: 125,
                readable_high_contrast: false,
            })
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_preference_write_retries_after_the_backoff() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("factory-ui-retry-{unique}"));
        fs::write(&root, "temporarily blocking the settings directory").unwrap();

        let path = root.join("ui-settings.ron");
        let preferences = UiPreferences {
            scale_percent: 150,
            readable_high_contrast: true,
            settings_path: path.clone(),
        };
        let mut app = App::new();
        app.init_resource::<Time<Real>>()
            .insert_resource(preferences)
            .init_resource::<UiPreferencesPersistenceState>()
            .add_systems(Update, save_ui_preferences_if_changed);

        app.update();
        assert!(!path.exists());
        assert!(
            app.world()
                .resource::<UiPreferencesPersistenceState>()
                .retry_after
                .is_some()
        );

        fs::remove_file(&root).unwrap();
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(PERSISTENCE_RETRY_DELAY);
        app.update();

        assert_eq!(
            read_ui_preferences_file(&path),
            Some(UiPreferencesFile {
                scale_percent: 150,
                readable_high_contrast: true,
            })
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn contrast_palette_preserves_transparency_and_increases_separation() {
        assert_eq!(high_contrast_background(Color::NONE), Color::NONE);
        let text = high_contrast_text(Color::srgb(0.5, 0.5, 0.5)).to_srgba();
        let background = high_contrast_background(Color::srgb(0.2, 0.2, 0.2)).to_srgba();
        assert!(text.red - background.red > 0.8);
    }
}
