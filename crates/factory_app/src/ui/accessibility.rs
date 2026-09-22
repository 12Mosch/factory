use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy::sprite::Text2dShadow;
use bevy::window::PrimaryWindow;
use serde::{Deserialize, Serialize};

use crate::map::resources::MapOverlay;
use crate::save_load::SaveLoadConfig;
use crate::ui::settings::{SettingsTab, SettingsWindowState};

pub const MIN_UI_SCALE_PERCENT: u16 = 75;
pub const MAX_UI_SCALE_PERCENT: u16 = 200;
const UI_SCALE_STEP_PERCENT: u16 = 25;
const MIN_LOGICAL_VIEWPORT_WIDTH: f32 = 800.0;
const MIN_LOGICAL_VIEWPORT_HEIGHT: f32 = 450.0;
const PERSISTENCE_RETRY_DELAY: Duration = Duration::from_secs(1);
/// Version of the persisted accessibility file. Legacy files without a version
/// predate reduced-motion and status-symbol options and upgrade in place.
pub const ACCESSIBILITY_PREFS_VERSION: u32 = 1;
/// Minimum pointer hit target for accessibility controls. Matches the 44px
/// WCAG guideline so touch, pen, and imprecise pointer input stay usable.
pub const MIN_ACCESSIBLE_HIT_TARGET_PX: f32 = 44.0;

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct UiPreferences {
    pub scale_percent: u16,
    pub readable_high_contrast: bool,
    pub reduced_motion: bool,
    pub status_symbols: bool,
    settings_path: PathBuf,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            scale_percent: 100,
            readable_high_contrast: false,
            reduced_motion: false,
            status_symbols: true,
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
    pub version: u32,
    pub scale_percent: u16,
    pub readable_high_contrast: bool,
    pub reduced_motion: bool,
    pub status_symbols: bool,
}

impl Default for UiPreferencesFile {
    fn default() -> Self {
        Self {
            version: ACCESSIBILITY_PREFS_VERSION,
            scale_percent: 100,
            readable_high_contrast: false,
            reduced_motion: false,
            status_symbols: true,
        }
    }
}

impl UiPreferencesFile {
    /// Builds the stable on-disk representation of the current preferences.
    pub fn from_preferences(preferences: &UiPreferences) -> Self {
        Self {
            version: ACCESSIBILITY_PREFS_VERSION,
            scale_percent: preferences
                .scale_percent
                .clamp(MIN_UI_SCALE_PERCENT, MAX_UI_SCALE_PERCENT),
            readable_high_contrast: preferences.readable_high_contrast,
            reduced_motion: preferences.reduced_motion,
            status_symbols: preferences.status_symbols,
        }
    }

    /// Sanitizes values loaded from files created by older or edited versions.
    fn normalize(&mut self) {
        if self.version != ACCESSIBILITY_PREFS_VERSION && self.version != 0 {
            *self = Self::default();
            return;
        }
        // Version 0 predates versioning: keep the stored scale/contrast and
        // fill the newer options with safe defaults.
        if self.version == 0 {
            self.version = ACCESSIBILITY_PREFS_VERSION;
        }
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

#[derive(Component)]
pub struct ReducedMotionButton;

#[derive(Component)]
pub struct StatusSymbolsButton;

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
type ReducedMotionButtonQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<ReducedMotionButton>,
    ),
>;
type StatusSymbolsButtonQuery<'w, 's> = Query<
    'w,
    's,
    &'static Interaction,
    (
        Changed<Interaction>,
        With<Button>,
        With<StatusSymbolsButton>,
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
    pub reduced_motion: bool,
    pub status_symbols: bool,
}

/// Applies Display and Accessibility button presses to the pending session.
pub(crate) fn handle_accessibility_settings_buttons(
    mut scale_buttons: UiScaleButtonQuery,
    mut contrast_buttons: ContrastButtonQuery,
    mut reduced_motion_buttons: ReducedMotionButtonQuery,
    mut status_symbol_buttons: StatusSymbolsButtonQuery,
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
        for interaction in &mut reduced_motion_buttons {
            if *interaction == Interaction::Pressed {
                window.pending_values.reduced_motion = !window.pending_values.reduced_motion;
                window.dirty = true;
            }
        }
        for interaction in &mut status_symbol_buttons {
            if *interaction == Interaction::Pressed {
                window.pending_values.status_symbols = !window.pending_values.status_symbols;
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
}

/// Spawns the accessibility controls for the Accessibility tab.
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
    spawn_accessibility_toggle(
        parent,
        if snapshot.readable_high_contrast {
            "ON"
        } else {
            "OFF"
        },
        AccessibilityToggle::HighContrast,
        snapshot.readable_high_contrast,
    );

    spawn_heading(parent, "Reduce motion and flashing");
    parent.spawn((
        Text::new("Disables nonessential animation smoothing such as rocket-rise interpolation. The simulation itself is unchanged."),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.72, 0.76, 0.69)),
    ));
    spawn_accessibility_toggle(
        parent,
        if snapshot.reduced_motion { "ON" } else { "OFF" },
        AccessibilityToggle::ReducedMotion,
        snapshot.reduced_motion,
    );

    spawn_heading(parent, "Status symbols");
    parent.spawn((
        Text::new("Prefixes machine, threat, signal, and build status with text symbols so state never depends on color alone."),
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.72, 0.76, 0.69)),
    ));
    spawn_accessibility_toggle(
        parent,
        if snapshot.status_symbols { "ON" } else { "OFF" },
        AccessibilityToggle::StatusSymbols,
        snapshot.status_symbols,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessibilityToggle {
    HighContrast,
    ReducedMotion,
    StatusSymbols,
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
            min_height: Val::Px(MIN_ACCESSIBLE_HIT_TARGET_PX),
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

/// Spawns an accessibility toggle with a 44px hit target and its marker.
fn spawn_accessibility_toggle(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    toggle: AccessibilityToggle,
    selected: bool,
) {
    let mut button = parent.spawn((
        Button,
        Node {
            min_width: Val::Px(52.0),
            min_height: Val::Px(MIN_ACCESSIBLE_HIT_TARGET_PX),
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
    match toggle {
        AccessibilityToggle::HighContrast => {
            button.insert(ReadableHighContrastButton);
        }
        AccessibilityToggle::ReducedMotion => {
            button.insert(ReducedMotionButton);
        }
        AccessibilityToggle::StatusSymbols => {
            button.insert(StatusSymbolsButton);
        }
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
    preferences.reduced_motion = file.reduced_motion;
    preferences.status_symbols = file.status_symbols;
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
        .unwrap_or_else(|_| {
            "(version:1,scale_percent:100,readable_high_contrast:false,reduced_motion:false,status_symbols:true)"
                .to_string()
        });
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

/// Reports whether a control meets the minimum accessible hit target.
pub fn accessible_hit_target_met(width_px: f32, height_px: f32) -> bool {
    width_px >= MIN_ACCESSIBLE_HIT_TARGET_PX && height_px >= MIN_ACCESSIBLE_HIT_TARGET_PX - 8.0
}

/// Collapses frame-interpolation motion when reduced motion is enabled.
/// The simulation tick is unchanged; only presentation smoothing is skipped.
pub fn reduced_motion_overstep(reduced_motion: bool, overstep: f32) -> f32 {
    if reduced_motion {
        0.0
    } else {
        overstep.clamp(0.0, 1.0)
    }
}

/// Relative luminance used to verify status colors stay separable without hue.
pub fn relative_luminance(color: Color) -> f32 {
    let source = color.to_srgba();
    0.2126 * source.red + 0.7152 * source.green + 0.0722 * source.blue
}

/// Two status colors are distinguishable when their luminance differs enough
/// to survive common color-vision deficiencies even if hues merge.
pub fn status_colors_distinguishable(first: Color, second: Color) -> bool {
    (relative_luminance(first) - relative_luminance(second)).abs() > 0.08
}

/// Shape/text alternative for a machine status. All tags are distinct ASCII so
/// state never depends on green/amber/red hue alone.
pub fn machine_status_symbol(status: factory_sim::MachineStatus) -> &'static str {
    use factory_sim::MachineStatus as Status;
    match status {
        Status::Working => "[>]",
        Status::Idle => "[=]",
        Status::NoRecipe => "[R?]",
        Status::NoResearch => "[T?]",
        Status::NoFuel => "[F!]",
        Status::NoPower => "[P!]",
        Status::NoInput => "[I!]",
        Status::NoFluid => "[W!]",
        Status::NoHeat => "[H!]",
        Status::OutputFull => "[X]",
    }
}

/// Prefixes machine guidance with its symbol when symbols are enabled.
pub fn format_accessible_machine_status(
    status: factory_sim::MachineStatus,
    guidance: &str,
    symbols_enabled: bool,
) -> String {
    if symbols_enabled {
        format!("{} {guidance}", machine_status_symbol(status))
    } else {
        guidance.to_string()
    }
}

/// Shape/text alternative for a threat alert. All tags are distinct ASCII.
pub fn threat_alert_glyph(kind: factory_sim::ThreatEventKind) -> &'static str {
    use factory_sim::ThreatEventKind as Kind;
    match kind {
        Kind::PollutionContact => "[~]",
        Kind::RaidPreparing => "[!]",
        Kind::RaidLaunched => "[!!]",
        Kind::StructureUnderAttack => "[X]",
        Kind::ExpansionSpotted => "[?]",
        Kind::BaseDestroyed => "[+]",
    }
}

/// Prefixes a threat label with its glyph when symbols are enabled.
pub fn format_accessible_threat_label(
    kind: factory_sim::ThreatEventKind,
    label: &str,
    symbols_enabled: bool,
) -> String {
    if symbols_enabled {
        format!("{} {label}", threat_alert_glyph(kind))
    } else {
        label.to_string()
    }
}

/// Shape/text alternative for a rail-signal aspect.
pub fn rail_signal_glyph(aspect: factory_sim::RailSignalAspect) -> &'static str {
    use factory_sim::RailSignalAspect as Aspect;
    match aspect {
        Aspect::Clear => "[GO]",
        Aspect::Reserved => "[WAIT]",
        Aspect::Blocked => "[STOP]",
    }
}

/// Human-readable rail-signal state that does not depend on lamp hue.
pub fn rail_signal_accessible_label(aspect: factory_sim::RailSignalAspect) -> &'static str {
    use factory_sim::RailSignalAspect as Aspect;
    match aspect {
        Aspect::Clear => "Clear",
        Aspect::Reserved => "Caution",
        Aspect::Blocked => "Stop",
    }
}

/// Shape/text alternative for a circuit wire color.
pub fn circuit_wire_glyph(color: factory_sim::WireColor) -> &'static str {
    match color {
        factory_sim::WireColor::Red => "[R]",
        factory_sim::WireColor::Green => "[G]",
    }
}

/// Shape/text alternative for a map overlay toggle.
pub fn map_overlay_glyph(overlay: MapOverlay) -> &'static str {
    match overlay {
        MapOverlay::Pollution => "[P]",
        MapOverlay::Resources => "[R]",
        MapOverlay::PowerNetworks => "[E]",
        MapOverlay::ProductionProblems => "[!]",
        MapOverlay::Enemies => "[X]",
        MapOverlay::ConstructionPlans => "[C]",
    }
}

/// Shape/text alternative for build validity. Valid and invalid never share a tag.
pub fn build_validity_glyph(is_valid: bool) -> &'static str {
    if is_valid { "[OK]" } else { "[X]" }
}

/// Prefixes build status text with its validity tag when symbols are enabled.
pub fn format_accessible_build_status(
    is_valid: bool,
    message: &str,
    symbols_enabled: bool,
) -> String {
    if symbols_enabled {
        format!("{} {message}", build_validity_glyph(is_valid))
    } else {
        message.to_string()
    }
}

/// Border width for selection states. Selected slots draw thicker so selection
/// never depends on border hue alone.
pub fn selection_border_width_px(selected: bool) -> f32 {
    if selected { 3.0 } else { 1.0 }
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
            version: ACCESSIBILITY_PREFS_VERSION,
            scale_percent: 175,
            readable_high_contrast: true,
            reduced_motion: true,
            status_symbols: false,
        };
        write_ui_preferences_file(&path, &file).unwrap();
        assert_eq!(read_ui_preferences_file(&path), Some(file));

        fs::write(&path, "(scale_percent:125)").unwrap();
        assert_eq!(
            read_ui_preferences_file(&path),
            Some(UiPreferencesFile {
                version: ACCESSIBILITY_PREFS_VERSION,
                scale_percent: 125,
                readable_high_contrast: false,
                reduced_motion: false,
                status_symbols: true,
            })
        );

        fs::write(&path, "(version:999,scale_percent:125)").unwrap();
        assert_eq!(
            read_ui_preferences_file(&path),
            Some(UiPreferencesFile::default())
        );

        fs::write(&path, "(version:1,scale_percent:10,reduced_motion:true)").unwrap();
        assert_eq!(
            read_ui_preferences_file(&path),
            Some(UiPreferencesFile {
                version: ACCESSIBILITY_PREFS_VERSION,
                scale_percent: MIN_UI_SCALE_PERCENT,
                readable_high_contrast: false,
                reduced_motion: true,
                status_symbols: true,
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
            reduced_motion: true,
            status_symbols: true,
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
                version: ACCESSIBILITY_PREFS_VERSION,
                scale_percent: 150,
                readable_high_contrast: true,
                reduced_motion: true,
                status_symbols: true,
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

    #[test]
    fn status_symbols_are_unique_per_domain() {
        use factory_sim::{MachineStatus, RailSignalAspect, ThreatEventKind, WireColor};
        use std::collections::HashSet;

        let machine = [
            MachineStatus::Working,
            MachineStatus::Idle,
            MachineStatus::NoRecipe,
            MachineStatus::NoResearch,
            MachineStatus::NoFuel,
            MachineStatus::NoPower,
            MachineStatus::NoInput,
            MachineStatus::NoFluid,
            MachineStatus::NoHeat,
            MachineStatus::OutputFull,
        ]
        .map(machine_status_symbol);
        assert_eq!(machine.iter().collect::<HashSet<_>>().len(), machine.len());

        let threats = [
            ThreatEventKind::PollutionContact,
            ThreatEventKind::RaidPreparing,
            ThreatEventKind::RaidLaunched,
            ThreatEventKind::StructureUnderAttack,
            ThreatEventKind::ExpansionSpotted,
            ThreatEventKind::BaseDestroyed,
        ]
        .map(threat_alert_glyph);
        assert_eq!(threats.iter().collect::<HashSet<_>>().len(), threats.len());

        let signals = [
            RailSignalAspect::Clear,
            RailSignalAspect::Reserved,
            RailSignalAspect::Blocked,
        ]
        .map(rail_signal_glyph);
        assert_eq!(signals.iter().collect::<HashSet<_>>().len(), signals.len());

        let wires = [WireColor::Red, WireColor::Green].map(circuit_wire_glyph);
        assert_ne!(wires[0], wires[1]);

        let overlays = MapOverlay::ALL.map(map_overlay_glyph);
        assert_eq!(
            overlays.iter().collect::<HashSet<_>>().len(),
            overlays.len()
        );

        assert_ne!(build_validity_glyph(true), build_validity_glyph(false));
    }

    #[test]
    fn accessible_formatting_prefixes_symbols_only_when_enabled() {
        use factory_sim::{MachineStatus, ThreatEventKind};

        assert_eq!(
            format_accessible_machine_status(MachineStatus::NoPower, "No power", true),
            "[P!] No power"
        );
        assert_eq!(
            format_accessible_machine_status(MachineStatus::NoPower, "No power", false),
            "No power"
        );
        assert_eq!(
            format_accessible_threat_label(ThreatEventKind::RaidLaunched, "Raid", true),
            "[!!] Raid"
        );
        assert_eq!(
            format_accessible_build_status(false, "Blocked", true),
            "[X] Blocked"
        );
        assert_eq!(
            format_accessible_build_status(true, "Ready", false),
            "Ready"
        );
    }

    #[test]
    fn reduced_motion_collapses_interpolation_and_hit_targets_meet_minimum() {
        assert_eq!(reduced_motion_overstep(true, 0.7), 0.0);
        assert_eq!(reduced_motion_overstep(false, 0.7), 0.7);
        assert!(accessible_hit_target_met(52.0, 44.0));
        assert!(!accessible_hit_target_met(32.0, 32.0));
        assert_eq!(
            MIN_ACCESSIBLE_HIT_TARGET_PX, 44.0,
            "hit targets follow the 44px guideline"
        );
    }

    #[test]
    fn newly_changed_ui_converges_to_high_contrast() {
        let normal = Color::srgb(0.5, 0.5, 0.5);
        let converted = high_contrast_text(normal);
        assert_eq!(high_contrast_text(converted), converted);
        assert!(status_colors_distinguishable(
            Color::srgb(0.42, 0.84, 0.55),
            Color::srgb(1.0, 0.30, 0.24)
        ));
    }
}
