use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::{SaveLoadError, load_from_bytes, save_to_bytes};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Instant, SystemTime};

use crate::build::resources::BuildPlacementState;
use crate::constants::SIM_TICKS_PER_SECOND;
use crate::map::resources::{MapTextureCache, MapViewState};
use crate::rendering::map_texture::MapTextureUploadQueue;
use crate::rendering::resource_cells::ResourceRenderCache;
use crate::rendering::resources::VisibleEntityIds;
use crate::resources::{SimAccessError, SimResource};
use crate::simulation::{SimCommandBacklog, SimCommandRequest, SimCommandResult};
use crate::ui::resources::OpenContainer;

pub const MANUAL_SAVE_SLOTS: [SaveSlotKind; 3] = [
    SaveSlotKind::Manual(1),
    SaveSlotKind::Manual(2),
    SaveSlotKind::Manual(3),
];

pub const LOAD_SAVE_SLOTS: [SaveSlotKind; 5] = [
    SaveSlotKind::Manual(1),
    SaveSlotKind::Manual(2),
    SaveSlotKind::Manual(3),
    SaveSlotKind::Quick,
    SaveSlotKind::Auto,
];

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadConfig {
    pub root_dir: PathBuf,
    pub autosave_interval_ticks: u64,
}

impl Default for SaveLoadConfig {
    fn default() -> Self {
        Self {
            root_dir: default_save_root(),
            autosave_interval_ticks: (5.0 * 60.0 * SIM_TICKS_PER_SECOND) as u64,
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadWindowState {
    pub open: bool,
    pub tab: SaveLoadTab,
    pub selected_slot: SaveSlotKind,
}

impl Default for SaveLoadWindowState {
    fn default() -> Self {
        Self {
            open: false,
            tab: SaveLoadTab::Save,
            selected_slot: SaveSlotKind::Manual(1),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SaveLoadTab {
    #[default]
    Save,
    Load,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveSlotKind {
    Manual(usize),
    Quick,
    Auto,
}

impl Default for SaveSlotKind {
    fn default() -> Self {
        Self::Manual(1)
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadStatus {
    pub message: Option<String>,
    pub kind: SaveLoadStatusKind,
    pub last_completed_slot: Option<SaveSlotKind>,
}

impl Default for SaveLoadStatus {
    fn default() -> Self {
        Self {
            message: None,
            kind: SaveLoadStatusKind::Info,
            last_completed_slot: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SaveLoadStatusKind {
    #[default]
    Info,
    Success,
    Error,
}

#[derive(Resource, Default)]
pub struct PendingSaveJobs {
    jobs: Vec<SaveJob>,
}

impl PendingSaveJobs {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub fn is_slot_pending(&self, slot: SaveSlotKind) -> bool {
        self.jobs.iter().any(|job| job.slot == slot)
    }

    pub fn any_running(&self) -> bool {
        !self.jobs.is_empty()
    }

    pub fn pending_slots(&self) -> Vec<SaveSlotKind> {
        self.jobs.iter().map(|job| job.slot).collect()
    }
}

struct SaveJob {
    slot: SaveSlotKind,
    explicit: bool,
    pollable: bool,
    handle: JoinHandle<Result<SaveJobOutcome, String>>,
}

#[derive(Debug)]
struct SaveJobOutcome {
    lock_wait_ms: f64,
    serialize_ms: f64,
    write_ms: f64,
    total_ms: f64,
    bytes: usize,
}

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SaveLoadMetrics {
    pub last_request_submission_ms: f64,
    pub last_worker_lock_wait_ms: f64,
    pub last_serialize_ms: f64,
    pub last_write_ms: f64,
    pub last_total_ms: f64,
    pub last_bytes: usize,
}

#[derive(Resource, Default)]
pub struct AutosaveState {
    pub last_autosave_tick: u64,
}

#[derive(Resource, Default)]
pub struct PresentationReloadToken {
    pub value: u64,
}

pub(crate) fn initialize_autosave_tick(sim: Res<SimResource>, mut autosave: ResMut<AutosaveState>) {
    autosave.last_autosave_tick = sim.read().tick_count();
}

pub fn slot_path(config: &SaveLoadConfig, slot: SaveSlotKind) -> PathBuf {
    config.root_dir.join(match slot {
        SaveSlotKind::Manual(1) => "slot_1.factsim",
        SaveSlotKind::Manual(2) => "slot_2.factsim",
        SaveSlotKind::Manual(3) => "slot_3.factsim",
        SaveSlotKind::Manual(_) => "slot_invalid.factsim",
        SaveSlotKind::Quick => "quicksave.factsim",
        SaveSlotKind::Auto => "autosave.factsim",
    })
}

pub fn slot_display_name(slot: SaveSlotKind) -> &'static str {
    match slot {
        SaveSlotKind::Manual(1) => "Slot 1",
        SaveSlotKind::Manual(2) => "Slot 2",
        SaveSlotKind::Manual(3) => "Slot 3",
        SaveSlotKind::Manual(_) => "Invalid Slot",
        SaveSlotKind::Quick => "Quicksave",
        SaveSlotKind::Auto => "Autosave",
    }
}

pub fn slot_exists(config: &SaveLoadConfig, slot: SaveSlotKind) -> bool {
    slot_path(config, slot).is_file()
}

pub fn slot_modified_label(config: &SaveLoadConfig, slot: SaveSlotKind) -> String {
    let path = slot_path(config, slot);
    let Ok(metadata) = fs::metadata(path) else {
        return "Empty".to_string();
    };
    let Ok(modified) = metadata.modified() else {
        return "Saved".to_string();
    };
    modified_time_label(modified)
}

pub fn request_save(
    slot: SaveSlotKind,
    sim: &SimResource,
    config: &SaveLoadConfig,
    pending_jobs: &mut PendingSaveJobs,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
) -> bool {
    request_save_with_status(slot, sim, config, pending_jobs, status, metrics, true)
}

pub(crate) fn handle_save_load_shortcuts(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    config: Res<SaveLoadConfig>,
    mut pending_jobs: ResMut<PendingSaveJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
) {
    let Some(keyboard) = keyboard else {
        return;
    };

    if keyboard.just_pressed(KeyCode::F5) {
        request_save(
            SaveSlotKind::Quick,
            &load_state.sim,
            &config,
            &mut pending_jobs,
            &mut status,
            &mut load_state.metrics,
        );
    }

    if keyboard.just_pressed(KeyCode::F9) {
        load_slot(
            SaveSlotKind::Quick,
            &config,
            &pending_jobs,
            &mut status,
            &mut load_state,
        );
    }
}

pub(crate) fn poll_save_jobs(
    mut pending_jobs: ResMut<PendingSaveJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut metrics: ResMut<SaveLoadMetrics>,
) {
    let mut index = 0;
    while index < pending_jobs.jobs.len() {
        if !pending_jobs.jobs[index].pollable {
            pending_jobs.jobs[index].pollable = true;
            index += 1;
            continue;
        }
        if !pending_jobs.jobs[index].handle.is_finished() {
            index += 1;
            continue;
        }

        let job = pending_jobs.jobs.swap_remove(index);
        let result = job
            .handle
            .join()
            .unwrap_or_else(|_| Err("save worker panicked".to_string()));
        match result {
            Ok(outcome) => {
                metrics.last_worker_lock_wait_ms = outcome.lock_wait_ms;
                metrics.last_serialize_ms = outcome.serialize_ms;
                metrics.last_write_ms = outcome.write_ms;
                metrics.last_total_ms = outcome.total_ms;
                metrics.last_bytes = outcome.bytes;
                if job.explicit || status.kind != SaveLoadStatusKind::Error {
                    status.message = Some(format!("{} saved.", slot_display_name(job.slot)));
                    status.kind = SaveLoadStatusKind::Success;
                    status.last_completed_slot = Some(job.slot);
                }
            }
            Err(error) => {
                status.message = Some(format!(
                    "Cannot save {}: {error}",
                    slot_display_name(job.slot)
                ));
                status.kind = SaveLoadStatusKind::Error;
                status.last_completed_slot = None;
            }
        }
    }
}

pub(crate) fn run_autosave(
    sim: Res<SimResource>,
    config: Res<SaveLoadConfig>,
    mut pending_jobs: ResMut<PendingSaveJobs>,
    mut autosave: ResMut<AutosaveState>,
    mut status: ResMut<SaveLoadStatus>,
    mut metrics: ResMut<SaveLoadMetrics>,
) {
    let tick = sim.read().tick_count();
    if tick
        < autosave
            .last_autosave_tick
            .saturating_add(config.autosave_interval_ticks)
    {
        return;
    }
    if pending_jobs.any_running() {
        return;
    }
    if request_save_with_status(
        SaveSlotKind::Auto,
        &sim,
        &config,
        &mut pending_jobs,
        &mut status,
        &mut metrics,
        false,
    ) {
        autosave.last_autosave_tick = tick;
    }
}

#[derive(SystemParam)]
pub(crate) struct LoadState<'w> {
    pub(crate) sim: ResMut<'w, SimResource>,
    pub(crate) window: ResMut<'w, SaveLoadWindowState>,
    pub(crate) autosave: ResMut<'w, AutosaveState>,
    pub(crate) build_state: ResMut<'w, BuildPlacementState>,
    pub(crate) open_container: ResMut<'w, OpenContainer>,
    pub(crate) map_cache: ResMut<'w, MapTextureCache>,
    pub(crate) map_uploads: ResMut<'w, MapTextureUploadQueue>,
    pub(crate) map_view: ResMut<'w, MapViewState>,
    pub(crate) resource_cache: ResMut<'w, ResourceRenderCache>,
    pub(crate) visible_entity_ids: ResMut<'w, VisibleEntityIds>,
    pub(crate) reload_token: ResMut<'w, PresentationReloadToken>,
    pub(crate) pending_commands: ResMut<'w, Messages<SimCommandRequest>>,
    pub(crate) pending_results: ResMut<'w, Messages<SimCommandResult>>,
    pub(crate) command_backlog: ResMut<'w, SimCommandBacklog>,
    pub(crate) metrics: ResMut<'w, SaveLoadMetrics>,
}

pub(crate) fn load_slot(
    slot: SaveSlotKind,
    config: &SaveLoadConfig,
    pending_jobs: &PendingSaveJobs,
    status: &mut SaveLoadStatus,
    state: &mut LoadState,
) -> bool {
    // A save worker holds a read lock on the simulation, so `replace` would
    // fail with `Busy` anyway. Bail out before the expensive file read and
    // deserialization instead of discovering the conflict after the work.
    if pending_jobs.any_running() {
        status.message = Some("Cannot load while a save is in progress.".to_string());
        status.kind = SaveLoadStatusKind::Error;
        status.last_completed_slot = None;
        return false;
    }

    let path = slot_path(config, slot);
    if !path.is_file() {
        status.message = Some(format!(
            "Cannot load {}: slot is empty.",
            slot_display_name(slot)
        ));
        status.kind = SaveLoadStatusKind::Error;
        status.last_completed_slot = None;
        return false;
    }

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            status.message = Some(format!(
                "Cannot load {}: failed to read save file ({error}).",
                slot_display_name(slot)
            ));
            status.kind = SaveLoadStatusKind::Error;
            status.last_completed_slot = None;
            return false;
        }
    };

    match load_from_bytes(&bytes) {
        Ok(loaded) => {
            let tick = loaded.tick_count();
            let (player_x, player_y) = loaded.player().position_tiles();
            if let Err(error) = state.sim.replace(loaded) {
                status.message = Some(match error {
                    SimAccessError::Busy => "Cannot load while a save is in progress.".to_string(),
                    SimAccessError::Poisoned => {
                        "Cannot load: simulation access failed.".to_string()
                    }
                });
                status.kind = SaveLoadStatusKind::Error;
                status.last_completed_slot = None;
                return false;
            }
            // Commands queued against the previous world must not apply to
            // the loaded one, and results already produced by this frame's
            // fixed tick (which ran before this `Update` system) must not be
            // read as feedback for the loaded world either.
            state.pending_commands.clear();
            state.pending_results.clear();
            state.command_backlog.0.clear();
            state.build_state.selected = None;
            state.build_state.last_status = Default::default();
            state.open_container.entity_id = None;
            state.window.open = false;
            state.autosave.last_autosave_tick = tick;
            *state.map_cache = MapTextureCache::default();
            state.map_uploads.commands.clear();
            state.map_view.center_tile = Vec2::new(player_x, player_y);
            state.map_view.zoom = 1.0;
            state.map_view.follow_player = true;
            *state.resource_cache = ResourceRenderCache::default();
            *state.visible_entity_ids = VisibleEntityIds::default();
            state.reload_token.value = state.reload_token.value.wrapping_add(1);

            status.message = Some(format!("{} loaded.", slot_display_name(slot)));
            status.kind = SaveLoadStatusKind::Success;
            status.last_completed_slot = Some(slot);
            true
        }
        Err(error) => {
            status.message = Some(format_save_load_error(error));
            status.kind = SaveLoadStatusKind::Error;
            status.last_completed_slot = None;
            false
        }
    }
}

pub fn format_save_load_error(error: SaveLoadError) -> String {
    match error {
        SaveLoadError::UnsupportedSaveVersion { found, supported } => format!(
            "Cannot load save: save version {found} is unsupported by this build; supported version is {supported}."
        ),
        SaveLoadError::UnsupportedPrototypeFormatVersion { found, supported } => format!(
            "Cannot load save: prototype format {found} is unsupported by this build; supported format is {supported}."
        ),
        SaveLoadError::PrototypeHashMismatch { .. } => {
            "Cannot load save: prototype data does not match this build.".to_string()
        }
        SaveLoadError::InvalidMagic { .. } => {
            "Cannot load save: file is not a Factory save.".to_string()
        }
        SaveLoadError::InvalidSimulationState(_) => {
            "Cannot load save: saved simulation state failed validation.".to_string()
        }
        SaveLoadError::Codec(_) => "Cannot load save: file is corrupt or incomplete.".to_string(),
    }
}

fn request_save_with_status(
    slot: SaveSlotKind,
    sim: &SimResource,
    config: &SaveLoadConfig,
    pending_jobs: &mut PendingSaveJobs,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
    explicit: bool,
) -> bool {
    if !matches!(
        slot,
        SaveSlotKind::Manual(1..=3) | SaveSlotKind::Quick | SaveSlotKind::Auto
    ) {
        status.message = Some("Cannot save: invalid save slot.".to_string());
        status.kind = SaveLoadStatusKind::Error;
        status.last_completed_slot = None;
        return false;
    }
    if pending_jobs.is_slot_pending(slot) {
        if explicit {
            status.message = Some(format!(
                "{} is already being saved.",
                slot_display_name(slot)
            ));
            status.kind = SaveLoadStatusKind::Info;
        }
        return false;
    }

    let submission_start = Instant::now();
    let sim = sim.clone_handle();
    let path = slot_path(config, slot);
    let handle = thread::spawn(move || {
        let worker_start = Instant::now();
        let lock_start = Instant::now();
        let sim = sim
            .read()
            .map_err(|_| "simulation lock poisoned".to_string())?;
        let lock_wait_ms = lock_start.elapsed().as_secs_f64() * 1000.0;
        let serialize_start = Instant::now();
        let bytes = save_to_bytes(&sim).map_err(|error| format!("{error:?}"))?;
        let serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;
        drop(sim);
        let write_start = Instant::now();
        write_save_bytes(&path, &bytes).map_err(|error| error.to_string())?;
        Ok(SaveJobOutcome {
            lock_wait_ms,
            serialize_ms,
            write_ms: write_start.elapsed().as_secs_f64() * 1000.0,
            total_ms: worker_start.elapsed().as_secs_f64() * 1000.0,
            bytes: bytes.len(),
        })
    });
    pending_jobs.jobs.push(SaveJob {
        slot,
        explicit,
        pollable: false,
        handle,
    });
    metrics.last_request_submission_ms = submission_start.elapsed().as_secs_f64() * 1000.0;

    if explicit {
        status.message = Some(format!("Saving {}...", slot_display_name(slot)));
        status.kind = SaveLoadStatusKind::Info;
        status.last_completed_slot = None;
    }
    true
}

fn write_save_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = sibling_with_suffix(path, "tmp");
    let backup_path = sibling_with_suffix(path, "bak");
    let _ = fs::remove_file(&temp_path);
    fs::write(&temp_path, bytes)?;

    if !path.exists() {
        return fs::rename(&temp_path, path);
    }

    let _ = fs::remove_file(&backup_path);
    fs::rename(path, &backup_path)?;
    match fs::rename(&temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup_path, path);
            Err(error)
        }
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("save.factsim");
    path.with_file_name(format!("{file_name}.{suffix}-{}", std::process::id()))
}

fn default_save_root() -> PathBuf {
    default_data_dir()
        .map(|data_dir| data_dir.join("factory").join("saves"))
        .unwrap_or_else(|| PathBuf::from("saves"))
}

#[cfg(target_os = "windows")]
fn default_data_dir() -> Option<PathBuf> {
    env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn default_data_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn default_data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
}

fn modified_time_label(modified: SystemTime) -> String {
    let Ok(elapsed) = SystemTime::now().duration_since(modified) else {
        return "Saved just now".to_string();
    };
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        "Saved just now".to_string()
    } else if seconds < 60 * 60 {
        format!("Saved {} min ago", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("Saved {} hr ago", seconds / (60 * 60))
    } else {
        format!("Saved {} days ago", seconds / (24 * 60 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_fails_fast_while_save_reader_is_active() {
        let mut resource = SimResource::new(factory_sim::Simulation::new_test_world(1));
        let before_hash = resource.read().state_hash();
        let handle = resource.clone_handle();
        let guard = handle.read().expect("save reader should acquire the lock");

        let result = resource.replace(factory_sim::Simulation::new_test_world(2));

        assert_eq!(result, Err(SimAccessError::Busy));
        drop(guard);
        assert_eq!(resource.read().state_hash(), before_hash);
    }
}
