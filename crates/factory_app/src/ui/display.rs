//! Desktop display preferences and reversible, real-time window previews.
use std::path::PathBuf;
use std::time::Duration;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::window::{
    Monitor, MonitorSelection, OnMonitor, PresentMode, PrimaryWindow, WindowMode, WindowPosition,
};
use serde::{Deserialize, Serialize};

use super::settings::{SettingsTab, SettingsWindowState, spawn_button};
use super::window_sync::{WindowRootQuery, sync_window};
use crate::save_load::{SaveLoadConfig, write_save_bytes};

const PREVIEW_DURATION: Duration = Duration::from_secs(15);
const SIZES: [(u32, u32); 4] = [(960, 540), (1280, 720), (1600, 900), (1920, 1080)];
const FRAME_LIMITS: [u16; 6] = [0, 30, 60, 90, 120, 144];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct DisplayPreferences {
    pub version: u32,
    pub borderless: bool,
    pub vsync: bool,
    /// Zero means unlimited. This caps presentation, never the fixed simulation clock.
    pub frame_limit: u16,
    /// Window client size in logical pixels; None automatically fits the display.
    pub window_size: Option<(u32, u32)>,
}

impl Default for DisplayPreferences {
    /// Starts in a fitted window with VSync and a conservative frame limit.
    fn default() -> Self {
        Self {
            version: 1,
            borderless: false,
            vsync: true,
            frame_limit: 60,
            window_size: None,
        }
    }
}

impl DisplayPreferences {
    /// Replaces unknown versions and unsupported values with safe defaults.
    pub fn normalize(mut self) -> Self {
        if self.version != 1 {
            return Self::default();
        }
        if !FRAME_LIMITS.contains(&self.frame_limit) {
            self.frame_limit = 60;
        }
        if self.window_size.is_some_and(|size| !SIZES.contains(&size)) {
            self.window_size = None;
        }
        self
    }
}

#[derive(Resource, Default)]
pub struct DisplayState {
    pub draft: DisplayPreferences,
    applied: DisplayPreferences,
    confirmed: DisplayPreferences,
    preview_started: Option<Instant>,
    path: PathBuf,
    saved: DisplayPreferences,
    retry_after: Option<Instant>,
    available_sizes: Vec<(u32, u32)>,
    monitor_bounds: Option<Vec2>,
    available: bool,
    status: String,
}

impl DisplayState {
    /// Applies safe preferences immediately and previews geometry until confirmed.
    pub fn apply(&mut self) {
        if !self.available || self.preview_started.is_some() {
            return;
        }
        let next = self.draft.normalize();
        if next.borderless != self.applied.borderless
            || next.window_size != self.applied.window_size
        {
            self.preview_started = Some(Instant::now());
            self.confirmed = DisplayPreferences {
                borderless: self.confirmed.borderless,
                window_size: self.confirmed.window_size,
                ..next
            };
        } else {
            self.confirmed = next;
        }
        self.applied = next;
        self.draft = next;
    }

    /// Stages default preferences without changing the current window.
    pub fn reset_draft(&mut self) {
        self.draft = DisplayPreferences::default();
    }

    /// Discards unsubmitted edits while retaining currently applied preferences.
    pub fn discard_draft(&mut self) {
        self.draft = self.applied;
    }

    /// Keeps a timely confirmation or restores confirmed geometry after rejection.
    fn resolve_preview(&mut self, keep: bool) {
        let Some(started) = self.preview_started.take() else {
            return;
        };
        if keep && started.elapsed() < PREVIEW_DURATION {
            self.confirmed = self.applied;
        } else {
            self.applied = self.confirmed;
        }
        self.draft = self.applied;
    }

    /// Captures the display controls and persistence status for retained UI updates.
    pub(crate) fn snapshot(&self) -> DisplaySnapshot {
        DisplaySnapshot {
            draft: self.draft,
            sizes: self.available_sizes.clone(),
            available: self.available,
            status: self.status.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplaySnapshot {
    draft: DisplayPreferences,
    sizes: Vec<(u32, u32)>,
    available: bool,
    status: String,
}

#[derive(Component, Clone, Copy)]
pub enum DisplayButton {
    Mode,
    Vsync,
    FrameLimit,
    Size,
    Keep,
    Revert,
}

type DisplayButtons<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static DisplayButton),
    (Changed<Interaction>, With<Button>),
>;

/// Handles global confirmation actions and stages edits from the Display tab.
pub(crate) fn handle_display_buttons(
    buttons: DisplayButtons,
    mut state: ResMut<DisplayState>,
    mut settings: ResMut<SettingsWindowState>,
) {
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            DisplayButton::Keep => {
                state.resolve_preview(true);
                continue;
            }
            DisplayButton::Revert => {
                state.resolve_preview(false);
                continue;
            }
            _ => {}
        }
        if !settings.open
            || settings.active_tab != SettingsTab::Display
            || !state.available
            || state.preview_started.is_some()
        {
            continue;
        }
        match action {
            DisplayButton::Mode => state.draft.borderless = !state.draft.borderless,
            DisplayButton::Vsync => state.draft.vsync = !state.draft.vsync,
            DisplayButton::FrameLimit => {
                let index = FRAME_LIMITS
                    .iter()
                    .position(|&n| n == state.draft.frame_limit)
                    .unwrap_or(2);
                state.draft.frame_limit = FRAME_LIMITS[(index + 1) % FRAME_LIMITS.len()];
            }
            DisplayButton::Size if !state.draft.borderless => {
                state.draft.window_size = match state.draft.window_size {
                    None => state.available_sizes.first().copied(),
                    Some(size) => state
                        .available_sizes
                        .iter()
                        .position(|&s| s == size)
                        .and_then(|i| state.available_sizes.get(i + 1).copied()),
                };
            }
            _ => {}
        }
        settings.dirty = true;
    }
}

/// Loads sanitized desktop preferences and reconfirms saved fullscreen geometry.
pub(crate) fn load_display_preferences(
    config: Res<SaveLoadConfig>,
    mut state: ResMut<DisplayState>,
) {
    if cfg!(target_arch = "wasm32") {
        return;
    }
    state.path = config.root_dir.join("display-settings.ron");
    let loaded = std::fs::read_to_string(&state.path)
        .ok()
        .and_then(|text| ron::from_str::<DisplayPreferences>(&text).ok())
        .unwrap_or_default()
        .normalize();
    state.saved = loaded;
    state.draft = loaded;
    state.applied = loaded;
    // Reconfirm fullscreen on launch: display topology may have changed since saving.
    if loaded.borderless {
        state.confirmed = DisplayPreferences {
            borderless: false,
            window_size: None,
            ..loaded
        };
        state.preview_started = Some(Instant::now());
    } else {
        state.confirmed = loaded;
    }
}

/// Fits logical client dimensions inside the monitor, reserving space for decorations.
fn fitted_size(requested: Option<(u32, u32)>, bounds: Vec2) -> Vec2 {
    let requested = requested.unwrap_or((1280, 720));
    Vec2::new(requested.0 as f32, requested.1 as f32).min(bounds)
}

type MainWindow<'w, 's> =
    Query<'w, 's, (&'static mut Window, Option<&'static OnMonitor>), With<PrimaryWindow>>;

/// Reverts expired previews and applies preferences within the current monitor bounds.
pub(crate) fn sync_display(
    mut state: ResMut<DisplayState>,
    settings: Res<SettingsWindowState>,
    mut windows: MainWindow,
    monitors: Query<&Monitor>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut last_applied: Local<Option<(DisplayPreferences, Vec2)>>,
) {
    if state
        .preview_started
        .is_some_and(|start| start.elapsed() >= PREVIEW_DURATION)
        || keyboard.just_pressed(KeyCode::Escape)
    {
        state.resolve_preview(false);
    }
    if !settings.open {
        state.discard_draft();
    }
    let Ok((mut window, on_monitor)) = windows.single_mut() else {
        state.available = false;
        return;
    };
    state.available = !cfg!(target_arch = "wasm32");
    if !state.available {
        return;
    }
    let monitor = on_monitor.and_then(|m| monitors.get(m.0).ok());
    let bounds = monitor
        .map(|m| {
            (Vec2::new(m.physical_width as f32, m.physical_height as f32) / m.scale_factor as f32
                * 0.9)
                .max(Vec2::ONE)
        })
        .unwrap_or(Vec2::new(960.0, 540.0));
    let monitor_bounds = monitor.map(|_| bounds);
    if state.monitor_bounds != monitor_bounds {
        state.monitor_bounds = monitor_bounds;
        state.available_sizes.clear();
        if monitor.is_some() {
            state.available_sizes.extend(
                SIZES
                    .into_iter()
                    .filter(|&(w, h)| w as f32 <= bounds.x && h as f32 <= bounds.y),
            );
        }
    }
    if state
        .draft
        .window_size
        .is_some_and(|s| !state.available_sizes.contains(&s))
    {
        state.draft.window_size = None;
    }
    let applied = state.applied;
    let size = fitted_size(applied.window_size, bounds);
    if *last_applied != Some((applied, size)) {
        let geometry_changed = last_applied
            .is_none_or(|(old, old_size)| old.borderless != applied.borderless || old_size != size);
        window.present_mode = if applied.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };
        if geometry_changed {
            window.mode = if applied.borderless {
                WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            } else {
                WindowMode::Windowed
            };
            if !applied.borderless {
                window.resolution.set(size.x, size.y);
                window.position = WindowPosition::Centered(MonitorSelection::Current);
            }
        }
        *last_applied = Some((applied, size));
    }
}

/// Atomically saves confirmed preferences, retrying transient errors with backoff.
pub(crate) fn persist_display(mut state: ResMut<DisplayState>) {
    if state.path.as_os_str().is_empty()
        || state.preview_started.is_some()
        || state.saved == state.confirmed
        || state.retry_after.is_some_and(|t| Instant::now() < t)
    {
        return;
    }
    let result = ron::ser::to_string_pretty(&state.confirmed, ron::ser::PrettyConfig::default())
        .map_err(|error| error.to_string())
        .and_then(|text| {
            write_save_bytes(&state.path, text.as_bytes()).map_err(|error| error.to_string())
        });
    match result {
        Ok(durability) => {
            state.saved = state.confirmed;
            state.retry_after = None;
            if let Some(reason) = durability.degraded_reason() {
                // Committed but unsynced: keep the saved state (no retry that
                // could overwrite success) while reporting the degraded
                // barrier instead of a silent durable success.
                warn!("Display settings installed but not synced: {reason}");
                state.status =
                    format!("Display settings saved, but durability is degraded ({reason}).");
            } else {
                state.status.clear();
            }
        }
        Err(error) => {
            warn!("Could not save display settings: {error}");
            state.status = "Could not save display settings; retrying.".into();
            state.retry_after = Some(Instant::now() + Duration::from_secs(2));
        }
    }
}

/// Pace desktop frames using wall time, including rendering and VSync wait time.
/// A missed deadline starts a fresh interval instead of scheduling catch-up frames.
pub(crate) fn limit_frames(state: Res<DisplayState>, mut previous: Local<Option<Instant>>) {
    if !state.available || state.applied.frame_limit == 0 {
        *previous = None;
        return;
    }
    let interval = Duration::from_secs_f64(1.0 / f64::from(state.applied.frame_limit));
    if let Some(last) = *previous {
        let remaining = interval.saturating_sub(last.elapsed());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
    }
    *previous = Some(Instant::now());
}

#[derive(PartialEq)]
pub(crate) struct DisplayConfirmation;

/// Shows a centered confirmation modal for the duration of a geometry preview.
pub(crate) fn sync_display_confirmation(
    mut commands: Commands,
    state: Res<DisplayState>,
    mut roots: WindowRootQuery<DisplayConfirmation>,
) {
    sync_window(
        &mut commands,
        &mut roots,
        state.preview_started.is_some(),
        false,
        || DisplayConfirmation,
        || {
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::ZERO,
                    bottom: Val::ZERO,
                    left: Val::ZERO,
                    right: Val::ZERO,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                GlobalZIndex(10000),
            )
        },
        |parent, _| {
            parent
                .spawn((
                    Node {
                        width: Val::Percent(90.0),
                        max_width: Val::Px(480.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(24.0)),
                        row_gap: Val::Px(16.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.08, 0.1, 0.09)),
                    BorderColor::all(Color::srgb(0.40, 0.48, 0.36)),
                ))
                .with_children(|dialog| {
                    dialog.spawn((
                        Text::new("Keep display changes?"),
                        TextFont::from_font_size(20.0),
                    ));
                    dialog.spawn((
                        Text::new("Reverts after 15 seconds. Escape also reverts."),
                        TextFont::from_font_size(14.0),
                    ));
                    dialog
                        .spawn(Node {
                            justify_content: JustifyContent::FlexEnd,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(8.0),
                            row_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|actions| {
                            spawn_button(actions, "Keep", DisplayButton::Keep, true);
                            spawn_button(actions, "Revert", DisplayButton::Revert, false);
                        });
                });
        },
    );
}

/// Builds concise controls for the available desktop display preferences.
pub(crate) fn spawn_desktop_settings(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &DisplaySnapshot,
) {
    if !snapshot.available {
        parent.spawn(Text::new(
            "Desktop display controls require a native window.",
        ));
        return;
    }
    let p = snapshot.draft;
    spawn_button(
        parent,
        if p.borderless {
            "Mode: Borderless fullscreen"
        } else {
            "Mode: Windowed"
        },
        DisplayButton::Mode,
        false,
    );
    spawn_button(
        parent,
        if p.vsync {
            "VSync: On"
        } else {
            "VSync: Off (if supported)"
        },
        DisplayButton::Vsync,
        false,
    );
    let limit = if p.frame_limit == 0 {
        "Unlimited".into()
    } else {
        format!("{} FPS", p.frame_limit)
    };
    spawn_button(
        parent,
        &format!("Frame limit: {limit}"),
        DisplayButton::FrameLimit,
        false,
    );
    if !p.borderless {
        let size = p
            .window_size
            .map_or("Auto fit".into(), |(w, h)| format!("{w} × {h}"));
        spawn_button(
            parent,
            &format!("Window size: {size}"),
            DisplayButton::Size,
            false,
        );
    }
    if !snapshot.status.is_empty() {
        parent.spawn(Text::new(&snapshot.status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_preferences_round_trip_and_sanitize() {
        let value = DisplayPreferences {
            borderless: true,
            vsync: false,
            frame_limit: 144,
            window_size: Some((1920, 1080)),
            ..default()
        };
        assert_eq!(
            ron::from_str::<DisplayPreferences>(&ron::to_string(&value).unwrap())
                .unwrap()
                .normalize(),
            value
        );
        assert_eq!(
            ron::from_str::<DisplayPreferences>("()").unwrap(),
            DisplayPreferences::default()
        );
        assert_eq!(
            DisplayPreferences {
                version: 99,
                ..value
            }
            .normalize(),
            DisplayPreferences::default()
        );
        assert_eq!(
            DisplayPreferences {
                frame_limit: 1,
                window_size: Some((0, u32::MAX)),
                ..default()
            }
            .normalize(),
            DisplayPreferences::default()
        );
        assert!(ron::from_str::<DisplayPreferences>("broken").is_err());
    }

    #[test]
    fn risky_changes_require_confirmation_and_revert_to_last_confirmed() {
        let mut state = DisplayState {
            available: true,
            ..default()
        };
        state.draft.vsync = false;
        state.apply();
        assert!(!state.confirmed.vsync);
        assert!(state.preview_started.is_none());
        state.draft.borderless = true;
        state.apply();
        assert!(state.preview_started.is_some());
        assert!(!state.confirmed.borderless);
        state.resolve_preview(false);
        assert!(!state.applied.borderless);
        assert!(!state.applied.vsync);
        state.draft.borderless = true;
        state.apply();
        state.resolve_preview(true);
        assert!(state.confirmed.borderless);
        state.reset_draft();
        state.apply();
        state.resolve_preview(false);
        assert!(state.applied.borderless);
    }

    #[test]
    fn fitted_sizes_stay_inside_small_and_scaled_monitors() {
        assert_eq!(
            fitted_size(Some((1920, 1080)), Vec2::new(800.0, 450.0)),
            Vec2::new(800.0, 450.0)
        );
        assert_eq!(
            fitted_size(None, Vec2::new(3000.0, 2000.0)),
            Vec2::new(1280.0, 720.0)
        );
    }

    #[test]
    fn expired_confirmation_cannot_be_kept_after_a_stall() {
        let mut state = DisplayState {
            available: true,
            ..default()
        };
        state.draft.borderless = true;
        state.apply();
        state.preview_started = Some(Instant::now() - PREVIEW_DURATION);
        state.resolve_preview(true);
        assert_eq!(state.applied, DisplayPreferences::default());
    }

    /// Rejection and late confirmation roll back geometry without losing safe edits.
    #[test]
    fn rejected_geometry_keeps_vsync_and_frame_limit_changes() {
        for expired in [false, true] {
            let mut state = DisplayState {
                available: true,
                ..default()
            };
            state.draft = DisplayPreferences {
                borderless: true,
                vsync: false,
                frame_limit: 144,
                ..default()
            };
            state.apply();
            if expired {
                state.preview_started = Some(Instant::now() - PREVIEW_DURATION);
            }
            state.resolve_preview(expired);
            assert_eq!(
                state.applied,
                DisplayPreferences {
                    vsync: false,
                    frame_limit: 144,
                    ..default()
                }
            );
            assert_eq!(state.confirmed, state.applied);
            state.draft.frame_limit = 30;
            state.discard_draft();
            assert_eq!(state.draft, state.applied);
        }
    }

    #[test]
    fn persistence_only_writes_confirmed_values_and_reconfirms_fullscreen_on_launch() {
        let root = std::env::temp_dir().join(format!(
            "factory-display-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("display-settings.ron");
        let mut app = App::new();
        app.insert_resource(SaveLoadConfig {
            root_dir: root.clone(),
            ..default()
        })
        .init_resource::<DisplayState>()
        .add_systems(Startup, load_display_preferences)
        .add_systems(Update, persist_display);
        app.update();
        assert_eq!(
            app.world().resource::<DisplayState>().applied,
            DisplayPreferences::default()
        );
        {
            let mut state = app.world_mut().resource_mut::<DisplayState>();
            state.available = true;
            state.draft.borderless = true;
            state.draft.vsync = false;
            state.draft.frame_limit = 144;
            state.apply();
        }
        app.update();
        assert!(!path.exists(), "previews must not be persisted");
        app.world_mut()
            .resource_mut::<DisplayState>()
            .resolve_preview(true);
        app.update();
        let file =
            ron::from_str::<DisplayPreferences>(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(file.borderless);
        let mut relaunched = App::new();
        relaunched
            .insert_resource(SaveLoadConfig {
                root_dir: root.clone(),
                ..default()
            })
            .init_resource::<DisplayState>()
            .add_systems(Startup, load_display_preferences)
            .add_systems(Update, persist_display);
        relaunched.update();
        assert!(
            relaunched
                .world()
                .resource::<DisplayState>()
                .preview_started
                .is_some()
        );
        relaunched
            .world_mut()
            .resource_mut::<DisplayState>()
            .resolve_preview(false);
        relaunched.update();
        let recovered =
            ron::from_str::<DisplayPreferences>(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(!recovered.borderless);
        assert!(!recovered.vsync);
        assert_eq!(recovered.frame_limit, 144);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn monitor_dpi_filters_sizes_and_escape_restores_geometry() {
        let mut app = App::new();
        app.init_resource::<DisplayState>()
            .init_resource::<SettingsWindowState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, sync_display);
        let monitor = app
            .world_mut()
            .spawn(Monitor {
                name: None,
                physical_width: 3840,
                physical_height: 2160,
                physical_position: IVec2::ZERO,
                refresh_rate_millihertz: None,
                scale_factor: 2.0,
                video_modes: Vec::new(),
            })
            .id();
        let entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow, OnMonitor(monitor)))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<DisplayState>().available_sizes,
            vec![(960, 540), (1280, 720), (1600, 900)]
        );
        {
            let mut state = app.world_mut().resource_mut::<DisplayState>();
            state.draft.window_size = Some((1600, 900));
            state.draft.vsync = false;
            state.apply();
        }
        app.update();
        let window = app.world().entity(entity).get::<Window>().unwrap();
        assert_eq!(window.width(), 1600.0);
        assert_eq!(window.present_mode, PresentMode::AutoNoVsync);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let window = app.world().entity(entity).get::<Window>().unwrap();
        assert_eq!(window.width(), 1280.0);
        assert_eq!(window.present_mode, PresentMode::AutoNoVsync);
        app.world_mut()
            .entity_mut(monitor)
            .get_mut::<Monitor>()
            .unwrap()
            .physical_width = 1280;
        app.update();
        assert_eq!(
            app.world().entity(entity).get::<Window>().unwrap().width(),
            576.0
        );
        assert!(
            app.world()
                .resource::<DisplayState>()
                .available_sizes
                .is_empty()
        );
    }

    #[test]
    fn headless_window_preview_times_out_even_without_simulation_time() {
        let mut app = App::new();
        app.init_resource::<DisplayState>()
            .init_resource::<SettingsWindowState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, sync_display);
        let entity = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        {
            let mut state = app.world_mut().resource_mut::<DisplayState>();
            state.draft.borderless = true;
            state.apply();
        }
        app.update();
        assert!(matches!(
            app.world().entity(entity).get::<Window>().unwrap().mode,
            WindowMode::BorderlessFullscreen(_)
        ));
        app.world_mut()
            .resource_mut::<DisplayState>()
            .preview_started = Some(Instant::now() - PREVIEW_DURATION);
        app.update();
        assert_eq!(
            app.world().entity(entity).get::<Window>().unwrap().mode,
            WindowMode::Windowed
        );
        assert!(
            app.world()
                .resource::<DisplayState>()
                .preview_started
                .is_none()
        );
    }
}
