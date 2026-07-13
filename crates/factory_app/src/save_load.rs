mod catalog;
mod compatibility;
mod container;
mod jobs;
mod types;

pub use catalog::{refresh_catalog, scan_catalog};
pub use container::{
    CONTAINER_MAGIC, CONTAINER_VERSION, MAX_METADATA_BYTES, decode_container, encode_container,
};
pub use jobs::PendingSaveJobs;
pub use types::*;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::{SaveLoadError, load_from_bytes};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::build::resources::BuildPlacementState;
use crate::constants::SIM_TICKS_PER_SECOND;
use crate::map::resources::{MapDetailCache, MapTextureCache, MapViewState};
use crate::rendering::map_texture::MapTextureUploadQueue;
use crate::rendering::resource_cells::ResourceRenderCache;
use crate::rendering::resources::VisibleEntityIds;
use crate::resources::{SimAccessError, SimResource};
use crate::simulation::{SimCommandBacklog, SimCommandRequest, SimCommandResult};
use crate::ui::resources::OpenContainer;
use crate::world_setup::AppMode;

static MANUAL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadConfig {
    pub root_dir: PathBuf,
    pub autosave_interval_ticks: u64,
    pub autosave_slot_count: usize,
}

impl Default for SaveLoadConfig {
    fn default() -> Self {
        Self {
            root_dir: default_save_root(),
            autosave_interval_ticks: (5.0 * 60.0 * SIM_TICKS_PER_SECOND) as u64,
            autosave_slot_count: 5,
        }
    }
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

pub(crate) fn initialize_save_state(
    sim: Res<SimResource>,
    config: Res<SaveLoadConfig>,
    mut autosave: ResMut<AutosaveState>,
    mut catalog: ResMut<SaveCatalog>,
    mut status: ResMut<SaveLoadStatus>,
) {
    autosave.last_autosave_tick = sim.read().tick_count();
    refresh_with_status(&config, &mut catalog, &mut status);
}

pub(crate) fn refresh_catalog_on_manager_open(
    mut window: ResMut<SaveLoadWindowState>,
    config: Res<SaveLoadConfig>,
    mut catalog: ResMut<SaveCatalog>,
    mut status: ResMut<SaveLoadStatus>,
) {
    if window.open && window.refresh_on_open {
        refresh_with_status(&config, &mut catalog, &mut status);
        window.refresh_on_open = false;
    }
}

pub fn validate_save_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    let count = trimmed.chars().count();
    if count == 0 {
        return Err("Save name cannot be empty.".into());
    }
    if count > 64 {
        return Err("Save name must be at most 64 characters.".into());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("Save name cannot contain control characters.".into());
    }
    if trimmed.eq_ignore_ascii_case("quicksave") || trimmed.eq_ignore_ascii_case("autosave") {
        return Err("Quicksave and Autosave are reserved names.".into());
    }
    Ok(trimmed.to_string())
}

pub fn normalize_save_name(name: &str) -> String {
    name.trim().to_lowercase()
}

#[allow(clippy::too_many_arguments)]
pub fn request_named_save(
    name: &str,
    sim: &SimResource,
    config: &SaveLoadConfig,
    catalog: &SaveCatalog,
    pending: &mut PendingSaveJobs,
    confirmation: &mut PendingSaveConfirmation,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
) -> bool {
    let name = match validate_save_name(name) {
        Ok(name) => name,
        Err(error) => {
            set_error(status, error);
            return false;
        }
    };
    let normalized = normalize_save_name(&name);
    if let Some(existing) = catalog.named_case_insensitive(&name) {
        *confirmation = PendingSaveConfirmation::Overwrite(existing.id.clone());
        status.message = Some(format!("Overwrite {}?", existing.metadata.display_name));
        status.kind = SaveLoadStatusKind::Info;
        return false;
    }
    let (id, path) = generate_manual_target(config);
    jobs::queue_save(
        id,
        SaveKind::Named,
        name,
        path,
        Some(normalized),
        true,
        sim,
        pending,
        status,
        metrics,
    )
}

pub fn request_overwrite(
    id: &SaveId,
    sim: &SimResource,
    catalog: &SaveCatalog,
    pending: &mut PendingSaveJobs,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
) -> bool {
    let Some(entry) = catalog
        .get(id)
        .filter(|entry| entry.metadata.kind == SaveKind::Named)
    else {
        set_error(
            status,
            "Cannot overwrite: save is no longer in the catalog.",
        );
        return false;
    };
    jobs::queue_save(
        entry.id.clone(),
        SaveKind::Named,
        entry.metadata.display_name.clone(),
        entry.path.clone(),
        Some(normalize_save_name(&entry.metadata.display_name)),
        true,
        sim,
        pending,
        status,
        metrics,
    )
}

pub fn request_system_save(
    kind: SaveKind,
    sim: &SimResource,
    config: &SaveLoadConfig,
    pending: &mut PendingSaveJobs,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
    explicit: bool,
) -> bool {
    let (id, name) = match kind {
        SaveKind::Quicksave => (SaveId::new("quicksave"), "Quicksave".to_string()),
        SaveKind::Autosave { generation }
            if (1..=config.autosave_slot_count).contains(&generation) =>
        {
            (
                SaveId::new(format!("autosave-{generation}")),
                format!("Autosave {generation}"),
            )
        }
        _ => {
            set_error(status, "Cannot save: invalid system save target.");
            return false;
        }
    };
    let path = jobs::system_path(config, &kind);
    jobs::queue_save(
        id, kind, name, path, None, explicit, sim, pending, status, metrics,
    )
}

pub fn delete_save(
    id: &SaveId,
    config: &SaveLoadConfig,
    catalog: &mut SaveCatalog,
    pending: &PendingSaveJobs,
    status: &mut SaveLoadStatus,
) -> bool {
    let Some(entry) = catalog.get(id).cloned() else {
        set_error(status, "Cannot delete: save is no longer in the catalog.");
        refresh_with_status(config, catalog, status);
        return false;
    };
    if pending.is_id_pending(id) {
        set_error(status, "Cannot delete while this save is in progress.");
        return false;
    }
    let expected = expected_path(config, &entry);
    if entry.path != expected {
        set_error(status, "Cannot delete: catalog path validation failed.");
        return false;
    }
    if !entry.path.is_file() {
        set_error(status, "Cannot delete: save file is missing.");
        refresh_with_status(config, catalog, status);
        return false;
    }
    if let Err(error) = fs::remove_file(&entry.path) {
        set_error(
            status,
            format!("Cannot delete {}: {error}", entry.metadata.display_name),
        );
        return false;
    }
    status.message = Some(format!("{} deleted.", entry.metadata.display_name));
    status.kind = SaveLoadStatusKind::Success;
    status.last_completed_id = Some(id.clone());
    refresh_with_status(config, catalog, status);
    true
}

pub(crate) fn handle_save_load_shortcuts(
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
) {
    let Some(keyboard) = keyboard else {
        return;
    };
    if keyboard.just_pressed(KeyCode::F5) {
        request_system_save(
            SaveKind::Quicksave,
            &load_state.sim,
            &config,
            &mut pending,
            &mut status,
            &mut load_state.metrics,
            true,
        );
    }
    if keyboard.just_pressed(KeyCode::F9) {
        let id = SaveId::new("quicksave");
        load_save(&id, &catalog, &pending, &mut status, &mut load_state);
    }
}

pub(crate) fn poll_save_jobs(
    config: Res<SaveLoadConfig>,
    mut catalog: ResMut<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut metrics: ResMut<SaveLoadMetrics>,
) {
    for job in jobs::take_completed(&mut pending) {
        match job.result {
            Ok(outcome) => {
                metrics.last_worker_lock_wait_ms = outcome.lock_wait_ms;
                metrics.last_serialize_ms = outcome.serialize_ms;
                metrics.last_write_ms = outcome.write_ms;
                metrics.last_total_ms = outcome.total_ms;
                metrics.last_bytes = outcome.bytes;
                if job.explicit || status.kind != SaveLoadStatusKind::Error {
                    status.message = Some(format!("{} saved.", job.display_name));
                    status.kind = SaveLoadStatusKind::Success;
                    status.last_completed_id = Some(job.id);
                }
                refresh_with_status(&config, &mut catalog, &mut status);
            }
            Err(error) => {
                set_error(
                    &mut status,
                    format!("Cannot save {}: {error}", job.display_name),
                );
            }
        }
    }
}

pub(crate) fn run_autosave(
    sim: Res<SimResource>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut autosave: ResMut<AutosaveState>,
    mut status: ResMut<SaveLoadStatus>,
    mut metrics: ResMut<SaveLoadMetrics>,
) {
    let tick = sim.read().tick_count();
    if tick
        < autosave
            .last_autosave_tick
            .saturating_add(config.autosave_interval_ticks)
        || pending.any_running()
        || config.autosave_slot_count == 0
    {
        return;
    }
    let generation = choose_autosave_generation(&catalog, config.autosave_slot_count);
    if request_system_save(
        SaveKind::Autosave { generation },
        &sim,
        &config,
        &mut pending,
        &mut status,
        &mut metrics,
        false,
    ) {
        autosave.last_autosave_tick = tick;
    }
}

pub fn choose_autosave_generation(catalog: &SaveCatalog, count: usize) -> usize {
    for generation in 1..=count {
        if !catalog
            .entries()
            .iter()
            .any(|entry| entry.metadata.kind == SaveKind::Autosave { generation })
        {
            return generation;
        }
    }
    catalog
        .entries()
        .iter()
        .filter_map(|entry| match entry.metadata.kind {
            SaveKind::Autosave { generation } if generation <= count => {
                Some((entry.metadata.completed_at_unix_ms, generation))
            }
            _ => None,
        })
        .min()
        .map_or(1, |(_, generation)| generation)
}

#[derive(SystemParam)]
pub(crate) struct LoadState<'w> {
    pub(crate) next_mode: ResMut<'w, NextState<AppMode>>,
    pub(crate) sim: ResMut<'w, SimResource>,
    pub(crate) window: ResMut<'w, SaveLoadWindowState>,
    pub(crate) autosave: ResMut<'w, AutosaveState>,
    pub(crate) build_state: ResMut<'w, BuildPlacementState>,
    pub(crate) open_container: ResMut<'w, OpenContainer>,
    pub(crate) map_cache: ResMut<'w, MapTextureCache>,
    pub(crate) map_details: ResMut<'w, MapDetailCache>,
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

pub(crate) fn load_save(
    id: &SaveId,
    catalog: &SaveCatalog,
    pending: &PendingSaveJobs,
    status: &mut SaveLoadStatus,
    state: &mut LoadState,
) -> bool {
    if pending.any_running() {
        set_error(status, "Cannot load while a save is in progress.");
        return false;
    }
    let Some(entry) = catalog.get(id) else {
        set_error(status, "Cannot load: save is no longer in the catalog.");
        return false;
    };
    if !entry.compatibility.can_load() {
        set_error(
            status,
            entry
                .compatibility
                .reason()
                .unwrap_or_else(|| "Cannot load this save.".into()),
        );
        return false;
    }
    let bytes = match container::read_simulation_payload(&entry.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            set_error(
                status,
                format!("Cannot load {}: {error}", entry.metadata.display_name),
            );
            return false;
        }
    };
    match load_from_bytes(&bytes) {
        Ok(loaded) => {
            let tick = loaded.tick_count();
            let player_tile = loaded.player().position_tiles();
            if let Err(error) = state.sim.replace(loaded) {
                set_error(
                    status,
                    match error {
                        SimAccessError::Busy => "Cannot load while a save is in progress.",
                        SimAccessError::Poisoned => "Cannot load: simulation access failed.",
                    },
                );
                return false;
            }
            enter_swapped_world(state, tick, player_tile);
            status.message = Some(format!("{} loaded.", entry.metadata.display_name));
            status.kind = SaveLoadStatusKind::Success;
            status.last_completed_id = Some(id.clone());
            true
        }
        Err(error) => {
            set_error(status, format_save_load_error(error));
            false
        }
    }
}

pub(crate) fn enter_swapped_world(state: &mut LoadState, tick: u64, player_tile: (f32, f32)) {
    state.pending_commands.clear();
    state.pending_results.clear();
    state.command_backlog.0.clear();
    state.build_state.selected = None;
    state.build_state.last_status = Default::default();
    state.open_container.entity_id = None;
    state.window.open = false;
    state.autosave.last_autosave_tick = tick;
    *state.map_cache = MapTextureCache::default();
    state.map_details.clear();
    state.map_uploads.commands.clear();
    state.map_view.center_tile = Vec2::new(player_tile.0, player_tile.1);
    state.map_view.zoom = 1.0;
    state.map_view.follow_player = true;
    *state.resource_cache = ResourceRenderCache::default();
    *state.visible_entity_ids = VisibleEntityIds::default();
    state.reload_token.value = state.reload_token.value.wrapping_add(1);
    state.next_mode.set(AppMode::InGame);
}

pub fn format_save_load_error(error: SaveLoadError) -> String {
    match error {
        SaveLoadError::UnsupportedSaveVersion { found, supported } if found > supported => format!("Cannot load save: format {found} was created by a newer build; update the game."),
        SaveLoadError::UnsupportedSaveVersion { found, supported } => format!("Cannot load save: format {found} is older than {supported}; this build has no migration."),
        SaveLoadError::UnsupportedPrototypeFormatVersion { found, supported } if found > supported => format!("Cannot load save: prototype format {found} was created by a newer build; update the game."),
        SaveLoadError::UnsupportedPrototypeFormatVersion { found, supported } => format!("Cannot load save: prototype format {found} is older than {supported}; this build has no migration."),
        SaveLoadError::PrototypeHashMismatch { .. } => "Cannot load save: it uses different game/prototype data and may come from another build or data set.".into(),
        SaveLoadError::InvalidMagic { .. } => "Cannot load save: file is not a Factory save.".into(),
        SaveLoadError::InvalidSimulationState(_) => "Cannot load save: saved simulation state failed validation.".into(),
        SaveLoadError::Codec(_) => "Cannot load save: file is corrupt or incomplete.".into(),
    }
}

fn generate_manual_target(config: &SaveLoadConfig) -> (SaveId, PathBuf) {
    loop {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let counter = MANUAL_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = SaveId::new(format!(
            "manual-{nanos:x}-{:x}-{counter:x}",
            std::process::id()
        ));
        let path = config.root_dir.join(format!("{}.factsim", id.as_str()));
        if !path.exists() {
            return (id, path);
        }
    }
}

fn expected_path(config: &SaveLoadConfig, entry: &SaveEntry) -> PathBuf {
    match entry.metadata.kind {
        SaveKind::Named => config
            .root_dir
            .join(format!("{}.factsim", entry.id.as_str())),
        _ => jobs::system_path(config, &entry.metadata.kind),
    }
}

fn refresh_with_status(
    config: &SaveLoadConfig,
    catalog: &mut SaveCatalog,
    status: &mut SaveLoadStatus,
) {
    if let Err(error) = refresh_catalog(config, catalog) {
        set_error(status, format!("Cannot refresh save catalog: {error}"));
    }
}

fn set_error(status: &mut SaveLoadStatus, message: impl Into<String>) {
    status.message = Some(message.into());
    status.kind = SaveLoadStatusKind::Error;
    status.last_completed_id = None;
}

fn default_save_root() -> PathBuf {
    default_data_dir()
        .map(|dir| dir.join("factory").join("saves"))
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_names() {
        assert_eq!(validate_save_name("  Main Base  ").unwrap(), "Main Base");
        assert!(validate_save_name("").is_err());
        assert!(validate_save_name("QuIcKsAvE").is_err());
        assert!(validate_save_name(&"x".repeat(65)).is_err());
        assert!(validate_save_name("bad\nname").is_err());
        assert_eq!(normalize_save_name("ÉCLAIR"), normalize_save_name("éclair"));
    }

    #[test]
    fn autosave_selection_prefers_missing_then_oldest_with_generation_tie_break() {
        let mut catalog = SaveCatalog::default();
        let entries = [1usize, 2, 3, 4]
            .into_iter()
            .map(|generation| test_autosave_entry(generation, 100))
            .collect();
        catalog.replace(entries);
        assert_eq!(choose_autosave_generation(&catalog, 5), 5);

        let entries = (1usize..=5)
            .map(|generation| {
                test_autosave_entry(generation, if generation <= 2 { 50 } else { 100 })
            })
            .collect();
        catalog.replace(entries);
        assert_eq!(choose_autosave_generation(&catalog, 5), 1);
    }

    fn test_autosave_entry(generation: usize, timestamp: u64) -> SaveEntry {
        let id = SaveId::new(format!("autosave-{generation}"));
        SaveEntry {
            id: id.clone(),
            metadata: SaveMetadata {
                schema_version: 1,
                id,
                display_name: format!("Autosave {generation}"),
                kind: SaveKind::Autosave { generation },
                completed_at_unix_ms: timestamp,
                application_version: "test".into(),
            },
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: PathBuf::from(format!("autosave-{generation}.factsim")),
        }
    }
}
