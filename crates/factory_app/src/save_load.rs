mod catalog;
mod compatibility;
mod container;
mod freshness;
mod jobs;
pub mod lifecycle;
mod loads;
mod timestamp;
mod types;

pub(crate) use catalog::inspect::now_unix_ms;
#[cfg(test)]
pub(crate) use catalog::poll_catalog_scan;
pub(crate) use catalog::poll_catalog_validation_jobs;
pub use catalog::{PendingCatalogScan, refresh_catalog_blocking, scan_catalog};
pub(crate) use catalog::{poll_catalog_scan_system, request_catalog_scan};
pub(crate) use container::write_save_bytes;
pub use container::{
    BACKUP_ARTIFACT_MARKER, CONTAINER_MAGIC, CONTAINER_VERSION, MAX_METADATA_BYTES,
    METADATA_SCHEMA_VERSION, TEMP_ARTIFACT_MARKER, decode_container, encode_container,
    hold_save_artifact_lock_for_tests,
};
pub use jobs::PendingSaveJobs;
pub use lifecycle::{
    LoadJobError, LoadJobPhase, MAX_LOAD_WORKERS, MAX_QUEUED_LOADS, MAX_QUEUED_SAVES,
    MAX_RETAINED_SAVE_GENERATIONS, MAX_SAVE_WORKERS, PersistenceRequestId, SaveJobError,
    SaveJobPhase,
};
pub use loads::PendingLoadJobs;
pub(crate) use timestamp::local_datetime_from_unix_ms;
pub use types::*;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use factory_sim::SaveLoadError;
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::build::resources::BuildPlacementState;
use crate::constants::SIM_TICKS_PER_SECOND;
use crate::input::bindings::{ActionInput, InputAction};
use crate::input::panels::world_input_blocked;
use crate::input::resources::{AppInputState, TrainManualInput};
use crate::map::resources::{MapDetailCache, MapTextureCache, MapViewState};
use crate::rendering::map_texture::MapTextureUploadQueue;
use crate::rendering::resource_cells::ResourceRenderCache;
use crate::rendering::resources::VisibleEntityIds;
use crate::resources::{SimAccessError, SimResource};
use crate::simulation::{AppPauseState, SimCommandBacklog, SimCommandRequest, SimCommandResult};
use crate::ui::resources::{EquipmentWindowState, OpenContainer};
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
    pub last_snapshot_world_generation: u64,
    pub last_snapshot_capture_ms: f64,
    pub last_snapshot_tick: u64,
    pub last_snapshot_lock_wait_ms: f64,
    pub last_snapshot_lock_hold_ms: f64,
    pub last_snapshot_blocked_fixed_ticks: u64,
    pub last_snapshot_wire_bytes: usize,
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

/// Initializes autosave timing when a world exists and refreshes the disk catalog in all modes.
pub(crate) fn initialize_save_state(
    sim: Res<SimResource>,
    config: Res<SaveLoadConfig>,
    mut autosave: ResMut<AutosaveState>,
    mut catalog: ResMut<SaveCatalog>,
    mut status: ResMut<SaveLoadStatus>,
) {
    autosave.last_autosave_tick = if sim.is_initialized() {
        sim.read().tick_count()
    } else {
        0
    };
    if let Err(error) = refresh_catalog_blocking(&config, &mut catalog) {
        set_error(&mut status, format!("Cannot refresh save catalog: {error}"));
    }
}

pub(crate) fn refresh_catalog_on_manager_open(
    mut window: ResMut<SaveLoadWindowState>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    mut pending_scan: ResMut<PendingCatalogScan>,
) {
    if window.open && window.refresh_on_open {
        // The previously listed entries stay visible until the background
        // scan lands; no filesystem work happens on this frame.
        request_catalog_scan(&config, &mut pending_scan, catalog.scan_epoch);
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

/// A named-save request parked while a catalog refresh is outstanding.
/// Admission validates uniqueness against the in-memory catalog, so a save
/// admitted from a stale catalog could duplicate a display name that an
/// external actor added mid-scan. The validated name waits here until the
/// scan lands and is then re-validated against the fresh catalog. A scan
/// that fails instead fails the parked request explicitly: freshness was
/// never established, so draining against the stale catalog is forbidden.
#[derive(Resource, Default)]
pub struct DeferredNamedSave {
    pub(crate) name: Option<String>,
    /// A request dropped by a failed scan, retained until a later scan
    /// installs successfully or a new request for a save is accepted. A
    /// follow-up (or any later) scan that also fails re-reports this
    /// failure instead of replacing it with a generic refresh error, so
    /// the settled status stays connected to the lost user action. Never
    /// re-admitted: the drain only consumes the parked `name`.
    pub(crate) dropped_name: Option<String>,
}

/// Admits a named save only from a settled catalog when the name is new.
/// While a refresh scan is outstanding the in-memory entries may predate
/// external additions, so a request for an unknown name is parked (last one
/// wins) with an Info status instead of being admitted; the deferred drain
/// re-runs admission — including the uniqueness check — once the scan
/// lands. A name already in the catalog routes to overwrite confirmation
/// immediately: that path mints no new entry, so it cannot duplicate a
/// display name no matter what the outstanding scan later reports. Parking
/// and re-admission are purely in-memory: filesystem work stays on the scan
/// worker throughout.
#[allow(clippy::too_many_arguments)]
pub fn request_named_save_guarded(
    name: &str,
    sim: &SimResource,
    config: &SaveLoadConfig,
    catalog: &SaveCatalog,
    pending_scan: &PendingCatalogScan,
    deferred: &mut DeferredNamedSave,
    pending: &mut PendingSaveJobs,
    confirmation: &mut PendingSaveConfirmation,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
) -> bool {
    let valid = match validate_save_name(name) {
        Ok(valid) => valid,
        Err(error) => {
            set_error(status, error);
            return false;
        }
    };
    if catalog.named_case_insensitive(&valid).is_none() && !pending_scan.is_empty() {
        deferred.name = Some(valid.clone());
        status.message = Some(format!(
            "Refreshing save list; {valid} will be created once it lands."
        ));
        status.kind = SaveLoadStatusKind::Info;
        status.last_completed_id = None;
        return false;
    }
    // An accepted admission supersedes any dropped-request context: the
    // user re-issued their intent, so a later scan failure must not
    // re-report the older dropped save as uncreated. Without this,
    // retrying a dropped name whose write then commits would see its
    // success replaced by a false failure — inviting a further retry
    // that mints a duplicate. A rejected retry (full queue, name already
    // saving, overwrite confirmation) accepts nothing, so the context
    // must survive: the original save is still uncreated, and later
    // failures must keep reporting it truthfully.
    let admitted = request_named_save(
        name,
        sim,
        config,
        catalog,
        pending,
        confirmation,
        status,
        metrics,
    );
    if admitted {
        deferred.dropped_name = None;
    }
    admitted
}

pub fn request_overwrite(
    id: &SaveId,
    sim: &SimResource,
    catalog: &SaveCatalog,
    pending: &mut PendingSaveJobs,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
    deferred: &mut DeferredNamedSave,
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
    let admitted = jobs::queue_save(
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
    );
    // A confirmed overwrite supersedes any dropped-request context, same
    // as a new admission: the replacement was accepted, so a later scan
    // failure must not re-report the older dropped save as uncreated. A
    // failed queueing leaves the context intact.
    if admitted {
        deferred.dropped_name = None;
    }
    admitted
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
    delete_save_with_loads(id, config, catalog, pending, None, status)
}

#[allow(clippy::too_many_arguments)]
pub fn delete_save_with_loads(
    id: &SaveId,
    config: &SaveLoadConfig,
    catalog: &mut SaveCatalog,
    pending: &PendingSaveJobs,
    pending_loads: Option<&PendingLoadJobs>,
    status: &mut SaveLoadStatus,
) -> bool {
    let Some(entry) = catalog.get(id).cloned() else {
        set_error(status, "Cannot delete: save is no longer in the catalog.");
        return false;
    };
    if pending.is_id_pending(id) {
        set_error(status, "Cannot delete while this save is in progress.");
        return false;
    }
    if pending_loads.is_some_and(|loads| loads.is_id_pending(id)) {
        set_error(status, "Cannot delete while this save is loading.");
        return false;
    }
    let expected = expected_path(config, &entry);
    if entry.path != expected {
        set_error(status, "Cannot delete: catalog path validation failed.");
        return false;
    }
    if !entry.path.is_file() {
        // The catalog view is stale: the file is already gone. Prune the
        // entry immediately (bumping the scan epoch so an in-flight scan
        // cannot resurrect it) while still reporting the missing file.
        catalog.remove(id);
        set_error(status, "Cannot delete: save file is missing.");
        return false;
    }
    if let Err(error) = container::remove_save_and_artifacts(&entry.path) {
        set_error(
            status,
            format!("Cannot delete {}: {error}", entry.metadata.display_name),
        );
        return false;
    }
    // Drop the entry in-memory for immediate list consistency, including
    // validation cache pruning. No scan is needed: the artifacts are gone
    // and the remaining entries are untouched. The removal bumps the scan
    // epoch, so a background scan requested before the deletion is dropped
    // on landing instead of resurrecting the entry.
    catalog.remove(id);
    status.message = Some(format!("{} deleted.", entry.metadata.display_name));
    status.kind = SaveLoadStatusKind::Success;
    status.last_completed_id = Some(id.clone());
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_save_load_shortcuts(
    actions: ActionInput,
    input_state: Option<Res<AppInputState>>,
    config: Res<SaveLoadConfig>,
    catalog: Res<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut pending_loads: ResMut<PendingLoadJobs>,
    mut status: ResMut<SaveLoadStatus>,
    mut load_state: LoadState,
) {
    if world_input_blocked(input_state.as_deref()) {
        return;
    }
    if actions.just_pressed(InputAction::QuickSave) {
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
    if actions.just_pressed(InputAction::QuickLoad) {
        let id = SaveId::new("quicksave");
        load_save(&id, &catalog, &mut pending_loads, &mut status, &load_state);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn poll_save_jobs(
    config: Res<SaveLoadConfig>,
    mut catalog: ResMut<SaveCatalog>,
    mut pending: ResMut<PendingSaveJobs>,
    mut pending_scan: ResMut<PendingCatalogScan>,
    mut status: ResMut<SaveLoadStatus>,
    mut metrics: ResMut<SaveLoadMetrics>,
) {
    for job in jobs::take_completed(&mut pending) {
        match job.result {
            Ok(outcome) => {
                // Results carry request and world-generation ids: the worker
                // enforces them, so a committed snapshot always belongs to
                // the requested world and tick.
                debug_assert_eq!(outcome.request_id, job.request_id);
                let _ = outcome.requested_generation;
                let _ = outcome.requested_tick;
                // Install the committed save directly so admission sees it
                // before the next background scan lands: a repeated
                // same-name request offers overwrite confirmation instead
                // of minting a duplicate, and autosave rotation accounts
                // for the occupied slot. The follow-up scan re-observes
                // the file and queues validation for the pending entry.
                catalog.upsert_committed_save(SaveEntry {
                    id: job.id.clone(),
                    metadata: SaveMetadata {
                        schema_version: METADATA_SCHEMA_VERSION,
                        id: job.id.clone(),
                        display_name: job.display_name.clone(),
                        kind: job.kind.clone(),
                        completed_at_unix_ms: now_unix_ms(),
                        application_version: env!("CARGO_PKG_VERSION").into(),
                        world_seed: None,
                    },
                    compatibility: SaveCompatibility::ValidationPending,
                    metadata_available: true,
                    path: job.path.clone(),
                    inspected: None,
                });
                metrics.last_snapshot_world_generation = outcome.snapshot_world_generation;
                metrics.last_snapshot_capture_ms = outcome.snapshot_capture_ms;
                metrics.last_snapshot_tick = outcome.snapshot_tick;
                metrics.last_snapshot_lock_wait_ms = outcome.snapshot_lock_wait_ms;
                metrics.last_snapshot_lock_hold_ms = outcome.snapshot_lock_hold_ms;
                metrics.last_snapshot_blocked_fixed_ticks = outcome.snapshot_blocked_fixed_ticks;
                metrics.last_snapshot_wire_bytes = outcome.snapshot_wire_bytes;
                metrics.last_serialize_ms = outcome.serialize_ms;
                metrics.last_write_ms = outcome.write_ms;
                metrics.last_total_ms = outcome.total_ms;
                metrics.last_bytes = outcome.bytes;
                if job.explicit || status.kind != SaveLoadStatusKind::Error {
                    status.message = Some(format!("{} saved.", job.display_name));
                    status.kind = SaveLoadStatusKind::Success;
                    status.last_completed_id = Some(job.id);
                }
                // The new file lands in the list via a background scan;
                // validation for it is re-queued on install.
                request_catalog_scan(&config, &mut pending_scan, catalog.scan_epoch);
            }
            Err(SaveJobError::Cancelled) => {
                // A pre-commit cancellation is never reported as committed.
                // Autosave cancellations stay silent; explicit ones inform.
                if job.explicit {
                    status.message = Some(format!("{} cancelled.", job.display_name));
                    status.kind = SaveLoadStatusKind::Info;
                    status.last_completed_id = None;
                }
            }
            Err(SaveJobError::Stale) => {
                if job.explicit {
                    status.message =
                        Some(format!("{} superseded by a newer world.", job.display_name));
                    status.kind = SaveLoadStatusKind::Info;
                    status.last_completed_id = None;
                }
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
    if !sim.is_initialized() {
        return;
    }
    let tick = sim.read().tick_count();
    if tick
        < autosave
            .last_autosave_tick
            .saturating_add(config.autosave_interval_ticks)
        || config.autosave_slot_count == 0
    {
        return;
    }
    // No `any_running` gate: autosaves coalesce by target when a save is
    // running or queued, and drop under queue backpressure without an error
    // status. The interval still bounds steady-state frequency.
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
    pub(crate) app_pause: Res<'w, AppPauseState>,
    pub(crate) autosave: ResMut<'w, AutosaveState>,
    pub(crate) build_state: ResMut<'w, BuildPlacementState>,
    pub(crate) open_container: ResMut<'w, OpenContainer>,
    pub(crate) equipment_window: ResMut<'w, EquipmentWindowState>,
    pub(crate) map_cache: ResMut<'w, MapTextureCache>,
    pub(crate) map_details: ResMut<'w, MapDetailCache>,
    pub(crate) map_uploads: ResMut<'w, MapTextureUploadQueue>,
    pub(crate) map_view: ResMut<'w, MapViewState>,
    pub(crate) resource_cache: ResMut<'w, ResourceRenderCache>,
    pub(crate) visible_entity_ids: ResMut<'w, VisibleEntityIds>,
    pub(crate) reload_token: ResMut<'w, PresentationReloadToken>,
    pub(crate) train_input: ResMut<'w, TrainManualInput>,
    pub(crate) pending_commands: ResMut<'w, Messages<SimCommandRequest>>,
    pub(crate) pending_results: ResMut<'w, Messages<SimCommandResult>>,
    pub(crate) command_backlog: ResMut<'w, SimCommandBacklog>,
    pub(crate) metrics: ResMut<'w, SaveLoadMetrics>,
}

/// Requests an asynchronous load. File reading, decoding, and validation run
/// on a bounded background worker; installation happens in [`poll_load_jobs`]
/// at the controlled application boundary. Returns true when the request was
/// accepted (queued or started), false with an error status otherwise.
pub(crate) fn load_save(
    id: &SaveId,
    catalog: &SaveCatalog,
    pending: &mut PendingLoadJobs,
    status: &mut SaveLoadStatus,
    state: &LoadState,
) -> bool {
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
    // Loads run concurrently with saves: decoding needs no simulation lock.
    // Installation retries when the simulation is busy instead of failing.
    let observed_generation = if state.sim.is_initialized() {
        state.sim.replacement_revision()
    } else {
        0
    };
    loads::queue_load(
        entry.id.clone(),
        entry.metadata.display_name.clone(),
        entry.path.clone(),
        observed_generation,
        pending,
    );
    status.message = Some(format!("Loading {}...", entry.metadata.display_name));
    status.kind = SaveLoadStatusKind::Info;
    status.last_completed_id = None;
    true
}

/// Collects finished load workers and installs the newest still-current
/// candidate at the controlled boundary before map-texture and render sync.
///
/// Stale results (superseded by a newer load request, or observing a world
/// generation that has since been replaced by a newer installation) are
/// discarded without touching the active world. A candidate whose
/// installation meets a busy simulation is retained and retried next frame;
/// fixed ticks keep deferring via `try_write` and commands stay queued, so
/// input is neither discarded nor duplicated across the wait.
pub(crate) fn poll_load_jobs(
    mut pending: ResMut<PendingLoadJobs>,
    mut status: ResMut<SaveLoadStatus>,
    catalog: Res<SaveCatalog>,
    mut state: LoadState,
) {
    // Service an in-flight path confirmation first: with a same-round
    // verdict in hand it is one step from installation.
    if let Some(recertifying) = pending.take_recertifying()
        && !poll_recertifying_load(
            &mut pending,
            &mut status,
            &catalog,
            &mut state,
            recertifying,
        )
    {
        return;
    }
    if let Some(ready) = pending.take_ready_for_install() {
        // A newer request accepted while this candidate waited supersedes it:
        // installing now would put an obsolete world over the requested one.
        if pending
            .latest_request()
            .is_some_and(|latest| ready.request_id != latest)
        {
            // Superseded; drop without touching the world or reporting.
        } else if install_ready_load(&mut pending, &mut status, &catalog, &mut state, ready) {
            // Installed or terminally resolved; continue to completions.
        } else {
            // Busy: candidate retained inside `pending`; try again next frame.
            return;
        }
    }
    for completed in loads::take_completed_loads(&mut pending) {
        // Newest wins covers the whole request, including failure
        // reporting: an obsolete completion — success or error — is
        // discarded without touching the world or the shared status, so a
        // superseded failure can never overwrite the current request's
        // "Loading..." message while it still runs.
        if pending.completion_superseded(completed.request_id) {
            continue;
        }
        match completed.result {
            Ok(candidate) => {
                let current = current_generation(&state);
                if state.sim.is_initialized() && current != completed.observed_generation {
                    // A newer world was installed after this worker started
                    // (e.g. new-world creation). Discard without touching it.
                    // Load installs rebase queued observations, so a queued
                    // newer load is not stale after an older install. The
                    // orphaned "Loading..." status is released when this was
                    // the latest request and nothing remains.
                    clear_loading_status_if_idle(&pending, &mut status);
                    continue;
                }
                let ready = loads::ReadyLoad {
                    id: completed.id,
                    display_name: completed.display_name,
                    request_id: completed.request_id,
                    path: candidate.path.clone(),
                    candidate,
                };
                if !install_ready_load(&mut pending, &mut status, &catalog, &mut state, ready) {
                    return;
                }
            }
            Err(LoadJobError::Cancelled | LoadJobError::Stale) => {
                // Cancellation and superseding never touch the world and
                // never report success. A terminal end of the latest
                // request must still release an orphaned "Loading..."
                // status, or the manager keeps reporting a load forever
                // after all jobs finish (e.g. new-world creation via
                // `cancel_all`).
                clear_loading_status_if_idle(&pending, &mut status);
            }
            Err(LoadJobError::TransientIo(detail)) => {
                set_error(
                    &mut status,
                    format!(
                        "Cannot load {}: temporarily unreadable ({detail}); try again.",
                        completed.display_name
                    ),
                );
            }
            Err(LoadJobError::NotFound) => {
                set_error(
                    &mut status,
                    "Cannot load: save is no longer in the catalog.",
                );
            }
            Err(LoadJobError::Incompatible(reason)) => {
                set_error(&mut status, reason);
            }
            Err(error) => {
                set_error(
                    &mut status,
                    format!("Cannot load {}: {error}", completed.display_name),
                );
            }
        }
    }
}

/// Releases an orphaned "Loading..." status when no loads remain. Clears any
/// `Info` loading message — a specific `Loading {name}...` or the generic
/// install-wait message — so cancelling the latest load (e.g. starting a new
/// world mid-decode, including the queued-newest case where the removed newer
/// request named `latest_request`) never leaves the manager reporting a load
/// forever. Safe when idle: a live load always populates the queue, worker,
/// or retained candidate, so no current request can own the message; save
/// messages and error statuses are never touched.
pub(crate) fn clear_loading_status_if_idle(pending: &PendingLoadJobs, status: &mut SaveLoadStatus) {
    if !pending.is_empty() || status.kind != SaveLoadStatusKind::Info {
        return;
    }
    if status
        .message
        .as_deref()
        .is_some_and(|message| message.starts_with("Loading"))
    {
        status.message = None;
        status.last_completed_id = None;
    }
}

/// Re-queues an overtaken load so quickload converges on the committed
/// bytes once saves settle. Refreshes the "Loading..." status and reports
/// the request resolved-as-restarted.
fn restart_overtaken_load(
    pending: &mut PendingLoadJobs,
    status: &mut SaveLoadStatus,
    id: &SaveId,
    display_name: &str,
    path: &std::path::Path,
    observed_generation: u64,
) -> bool {
    loads::queue_load(
        id.clone(),
        display_name.to_owned(),
        path.to_path_buf(),
        observed_generation,
        pending,
    );
    status.message = Some(format!("Loading {display_name}..."));
    status.kind = SaveLoadStatusKind::Info;
    status.last_completed_id = None;
    true
}

/// Live world generation for rebasing a restarted load, or zero before the
/// first world exists.
fn current_generation(state: &LoadState) -> u64 {
    if state.sim.is_initialized() {
        state.sim.replacement_revision()
    } else {
        0
    }
}

/// Whether the candidate's decoded world is stale for the live world:
/// another system installed a newer world after the load observed its
/// generation. Checked at completion collection AND immediately before final
/// installation (both phases), so a candidate parked across frames can never
/// install over a newer world (#297: stale/out-of-order loads cannot
/// install). The pre-install check runs on the frame thread adjacent to the
/// installation with no other installer able to interleave, so the gate is
/// exact; the pre-confirmation check fails fast without spawning work.
fn candidate_world_stale(candidate: &loads::LoadCandidate, state: &LoadState) -> bool {
    state.sim.is_initialized() && current_generation(state) != candidate.observed_generation
}

/// Terminal catalog rejections evaluated at install time. The catalog may
/// change while a candidate waits or confirms, so both install phases
/// recheck: a removed entry is a terminal rejection (a save that is no
/// longer listed must not install), as is an entry that no longer loads.
/// Returns true when terminally resolved with the status set.
fn reject_stale_catalog_entry(
    status: &mut SaveLoadStatus,
    catalog: &SaveCatalog,
    ready: &loads::ReadyLoad,
) -> bool {
    let Some(entry) = catalog.get(&ready.id) else {
        set_error(status, "Cannot load: save is no longer in the catalog.");
        return true;
    };
    if !entry.compatibility.can_load() {
        set_error(
            status,
            entry
                .compatibility
                .reason()
                .unwrap_or_else(|| "Cannot load this save.".into()),
        );
        return true;
    }
    false
}

/// First install phase: gates a validated candidate and parks it for
/// same-round path confirmation. Returns true when resolved (terminally
/// rejected or parked for confirmation) and false when the candidate was
/// retained for retry.
///
/// The decode worker's certification is stale by the time the frame consumes
/// it whenever an external actor replaces the file in between, so the frame
/// never installs it directly: after the memory-only gates below pass, a
/// confirmation worker re-observes the path off-thread and the second phase
/// ([`poll_recertifying_load`]) installs only on its same-round verdict.
/// The artifact lock is held across the overtaken check and the confirmation
/// spawn, so a save commit cannot land between them: our writer bumps the
/// artifact epoch only while holding that lock. When the lock itself is held
/// (a save encoding or a scan in recovery), the candidate is retained
/// without spawning and retries next frame, then re-verifies freshness from
/// scratch — a confirmation earned before the wait must not outlive it.
fn install_ready_load(
    pending: &mut PendingLoadJobs,
    status: &mut SaveLoadStatus,
    catalog: &SaveCatalog,
    state: &mut LoadState,
    ready: loads::ReadyLoad,
) -> bool {
    let Some(_artifact_guard) = container::try_acquire_save_artifact_lock() else {
        // A save may be about to commit: retain and recheck next frame
        // instead of installing over it or stalling the frame.
        let waiting = status.kind != SaveLoadStatusKind::Error;
        pending.retain_ready(ready);
        if waiting {
            status.message = Some("Loading... waiting for the save to release the world.".into());
            status.kind = SaveLoadStatusKind::Info;
        }
        return false;
    };
    // A save committed after the worker opened its handle replaces the
    // target with bytes the candidate never decoded. An external actor can
    // likewise replace the path without bumping the process-local epoch;
    // the worker certifies the decoded instance off-thread and carries the
    // verdict in the candidate (no filesystem work happens on this frame
    // schedule). Installing either would roll the world back, so restart
    // the request instead: quickload converges on the current bytes once
    // saves settle. The epoch half is checked under the artifact lock, so
    // our own writer cannot commit between the check and the confirmation
    // spawn below.
    if ready.is_overtaken() || !ready.candidate.end_certified {
        drop(_artifact_guard);
        return restart_overtaken_load(
            pending,
            status,
            &ready.id,
            &ready.display_name,
            &ready.path,
            current_generation(state),
        );
    }
    // A newer world installed while the candidate waited or decoded: fail
    // fast here instead of spending a confirmation round on an install that
    // the pre-install gate would restart anyway.
    if candidate_world_stale(&ready.candidate, state) {
        drop(_artifact_guard);
        return restart_overtaken_load(
            pending,
            status,
            &ready.id,
            &ready.display_name,
            &ready.path,
            current_generation(state),
        );
    }
    // The catalog may have changed while decoding; rechecked after
    // confirmation as well, before anything installs.
    if reject_stale_catalog_entry(status, catalog, &ready) {
        return true;
    }
    pending.retain_recertifying(loads::RecertifyingLoad::begin(ready));
    false
}

/// Second install phase: consumes a parked path confirmation. Returns true
/// when resolved (installed, terminally rejected, or restarted) and false
/// when the candidate was parked again for retry.
///
/// Residual bound: a replacement landing between the confirmation worker's
/// final observation and the memory-only installation below still installs
/// obsolete bytes. That window holds no filesystem I/O or sleeps — the
/// frame joins the finished worker and installs in the same system run —
/// and a candidate retained across frames (busy simulation, held artifact
/// lock) is never installed on a previous round's verdict: it parks back to
/// the ready slot and earns a fresh confirmation instead.
fn poll_recertifying_load(
    pending: &mut PendingLoadJobs,
    status: &mut SaveLoadStatus,
    catalog: &SaveCatalog,
    state: &mut LoadState,
    recertifying: loads::RecertifyingLoad,
) -> bool {
    let loads::RecertifyingLoad { ready, confirm } = recertifying;
    // Every `latest_request` change clears the confirmation slot, so an
    // occupied slot always names the latest request.
    debug_assert!(
        pending
            .latest_request()
            .is_some_and(|latest| ready.request_id == latest)
    );
    let confirmation = match confirm.poll() {
        freshness::ConfirmationPoll::Pending => {
            pending.retain_recertifying(loads::RecertifyingLoad { ready, confirm });
            return false;
        }
        freshness::ConfirmationPoll::Ready(confirmation) => confirmation,
        freshness::ConfirmationPoll::WorkerGone => {
            // Pathological: the confirmation worker died. Restart re-decodes
            // and re-certifies from scratch instead of installing on no
            // evidence.
            return restart_overtaken_load(
                pending,
                status,
                &ready.id,
                &ready.display_name,
                &ready.path,
                current_generation(state),
            );
        }
    };
    if !confirmation.matched {
        // External replacement (or deletion) after the decode worker's
        // certification: restart converges on the current bytes once they
        // settle instead of installing the obsolete instance.
        return restart_overtaken_load(
            pending,
            status,
            &ready.id,
            &ready.display_name,
            &ready.path,
            current_generation(state),
        );
    }
    let Some(_artifact_guard) = container::try_acquire_save_artifact_lock() else {
        // The lock was held while confirming: park back to the ready slot
        // and earn a fresh confirmation next frame instead of installing on
        // this round's verdict.
        let waiting = status.kind != SaveLoadStatusKind::Error;
        pending.retain_ready(ready);
        if waiting {
            status.message = Some("Loading... waiting for the save to release the world.".into());
            status.kind = SaveLoadStatusKind::Info;
        }
        return false;
    };
    // Unbroken freshness chain under the artifact lock: no own-writer commit
    // between the decode worker's open, the confirmation's observation, and
    // this installation (our writer bumps the epoch only under this lock).
    // An external replacement in the confirmation-to-install window is ruled
    // out by `matched` above, up to the documented residual bound.
    if ready.candidate.artifact_epoch != confirmation.epoch
        || confirmation.epoch != container::save_artifact_epoch(&ready.path)
    {
        drop(_artifact_guard);
        return restart_overtaken_load(
            pending,
            status,
            &ready.id,
            &ready.display_name,
            &ready.path,
            current_generation(state),
        );
    }
    // World-generation gate: another system may have installed a newer world
    // while the candidate parked for confirmation. Installing now would
    // overwrite it with stale bytes, so restart against the live world
    // instead. Adjacent to the installation below with no other installer
    // able to interleave on the frame thread, this gate is exact.
    if candidate_world_stale(&ready.candidate, state) {
        drop(_artifact_guard);
        return restart_overtaken_load(
            pending,
            status,
            &ready.id,
            &ready.display_name,
            &ready.path,
            current_generation(state),
        );
    }
    if reject_stale_catalog_entry(status, catalog, &ready) {
        return true;
    }
    let loads::ReadyLoad {
        id,
        display_name,
        request_id,
        path: _,
        candidate,
    } = ready;
    // Copy scalar identity before the candidate moves into installation.
    let candidate_observed = candidate.observed_generation;
    let candidate_tick = candidate.tick;
    let candidate_player = candidate.player_tile;
    let candidate_path = candidate.path.clone();
    let candidate_epoch = candidate.artifact_epoch;
    let candidate_identity = candidate.observed_identity.clone();
    let candidate_certified = candidate.end_certified;
    // Two nested atomic steps: the artifact guard (held since the freshness
    // recheck) blocks our writer from committing between the check and this
    // `install`, which itself swaps the world and publishes the new
    // generation under a single write guard. Contention returns the
    // candidate so a busy simulation parks it for a fresh confirmation
    // instead of dropping a validated world.
    let installed = state.sim.install(candidate.simulation);
    // The world is swapped (or the attempt resolved); presentation reset
    // needs no artifact exclusion.
    drop(_artifact_guard);
    match installed {
        Ok(()) => {
            enter_swapped_world(state, candidate_tick, candidate_player);
            pending.note_installed(request_id);
            // An older install must not make a queued newer load look stale.
            let current = state.sim.replacement_revision();
            pending.rebase_queued_observations(current);
            status.message = Some(format!("{display_name} loaded."));
            status.kind = SaveLoadStatusKind::Success;
            status.last_completed_id = Some(id);
            true
        }
        Err(conflict) => match conflict.cause {
            SimAccessError::Busy => {
                // A save worker holds the read lock for capture. Park back
                // to the ready slot and earn a fresh confirmation next
                // frame; commands stay queued.
                pending.retain_ready(loads::ReadyLoad {
                    id,
                    display_name,
                    request_id,
                    path: candidate_path.clone(),
                    candidate: loads::LoadCandidate {
                        request_id,
                        observed_generation: candidate_observed,
                        simulation: *conflict.simulation,
                        tick: candidate_tick,
                        player_tile: candidate_player,
                        path: candidate_path,
                        artifact_epoch: candidate_epoch,
                        observed_identity: candidate_identity,
                        end_certified: candidate_certified,
                    },
                });
                if status.kind != SaveLoadStatusKind::Error {
                    status.message =
                        Some("Loading... waiting for the save to release the world.".into());
                    status.kind = SaveLoadStatusKind::Info;
                }
                false
            }
            SimAccessError::Poisoned => {
                set_error(status, "Cannot load: simulation access failed.");
                true
            }
        },
    }
}

pub(crate) fn map_load_error(error: SaveLoadError) -> LoadJobError {
    match error {
        SaveLoadError::TooLarge => LoadJobError::TooLarge,
        SaveLoadError::UnsupportedSaveVersion { .. }
        | SaveLoadError::UnsupportedPrototypeFormatVersion { .. }
        | SaveLoadError::PrototypeHashMismatch { .. } => {
            LoadJobError::Incompatible(format_save_load_error(error))
        }
        SaveLoadError::InvalidMagic { .. }
        | SaveLoadError::InvalidSimulationState(_)
        | SaveLoadError::Codec(_) => LoadJobError::Corrupt(format!("{error:?}")),
    }
}

pub(crate) fn enter_swapped_world(state: &mut LoadState, tick: u64, player_tile: (f32, f32)) {
    state.pending_commands.clear();
    state.pending_results.clear();
    state.command_backlog.0.clear();
    state.build_state.selected = None;
    state.build_state.last_status = Default::default();
    // Half-finished input belongs to the world it was aimed at. A driving press
    // waiting for the next fixed step names ids and tiles of the world being
    // replaced — and train ids are monotonic per world, so the same number in
    // the loaded one is a different train entirely.
    state.train_input.clear();
    state.open_container.close();
    *state.equipment_window = EquipmentWindowState::default();
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

/// Test-only helper exposing background validation classification for a single
/// path. Missing or momentarily unreadable files report
/// [`SaveCompatibility::ValidationPending`] (transient), never corruption.
#[doc(hidden)]
pub fn validate_loadable_file_for_tests(
    path: &std::path::Path,
    kind: &SaveKind,
    current_hash: u64,
) -> SaveCompatibility {
    let mut internal = std::collections::BTreeMap::new();
    catalog::validation::validate_loadable_file(path, kind, current_hash, &mut internal)
}

pub fn format_save_load_error(error: SaveLoadError) -> String {
    match error {
        SaveLoadError::TooLarge => "Cannot load save: it exceeds this build's save size or collection limits.".into(),
        SaveLoadError::UnsupportedSaveVersion { found, supported } if found > supported => format!("Cannot load save: format {found} was created by a newer build; update the game."),
        SaveLoadError::UnsupportedSaveVersion { found, .. } => format!("Cannot load save: format {found} predates the oldest supported migration source ({}). Use a build that supports it and re-save before updating.", factory_sim::OLDEST_SUPPORTED_SAVE_VERSION),
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
    fn overtaken_load_restarts_with_loading_status() {
        let mut pending = PendingLoadJobs::default();
        let mut status = SaveLoadStatus::default();
        let path = PathBuf::from("restarted.factsim");
        assert!(restart_overtaken_load(
            &mut pending,
            &mut status,
            &SaveId::new("quicksave"),
            "Quicksave",
            &path,
            7,
        ));
        // The same target is queued again (the worker may already have
        // failed on the missing file, but the request record persists
        // until collected) and the shared status keeps reporting the
        // running request, never an error or a stale success.
        assert_eq!(pending.pending_ids(), vec![SaveId::new("quicksave")],);
        assert!(pending.latest_request().is_some());
        assert_eq!(status.message.as_deref(), Some("Loading Quicksave..."),);
        assert_eq!(status.kind, SaveLoadStatusKind::Info);
    }

    #[test]
    fn cancelled_latest_load_releases_its_loading_status() {
        let pending = PendingLoadJobs::default();
        let mut status = SaveLoadStatus {
            message: Some("Loading Base...".into()),
            kind: SaveLoadStatusKind::Info,
            last_completed_id: None,
        };
        clear_loading_status_if_idle(&pending, &mut status);
        assert!(
            status.message.is_none(),
            "a terminal latest load must not leave its Loading status behind, got: {status:?}"
        );
    }

    #[test]
    fn loading_status_clearing_never_wipes_live_or_unrelated_messages() {
        // A queued newer load keeps the queue non-empty, so its message is
        // preserved even though another completion ended.
        let mut pending = PendingLoadJobs::default();
        loads::queue_load(
            SaveId::new("rescue"),
            "Rescue".into(),
            PathBuf::from("rescue.factsim"),
            0,
            &mut pending,
        );
        let mut status = SaveLoadStatus {
            message: Some("Loading Rescue...".into()),
            kind: SaveLoadStatusKind::Info,
            last_completed_id: None,
        };
        clear_loading_status_if_idle(&pending, &mut status);
        assert_eq!(status.message.as_deref(), Some("Loading Rescue..."));
        // The remaining checks use an idle queue.
        let idle = PendingLoadJobs::default();
        // Error statuses are never cleared.
        let mut status = SaveLoadStatus {
            message: Some("Loading Base...".into()),
            kind: SaveLoadStatusKind::Error,
            last_completed_id: None,
        };
        clear_loading_status_if_idle(&idle, &mut status);
        assert_eq!(status.message.as_deref(), Some("Loading Base..."));
        // Save messages are not loading messages.
        let mut status = SaveLoadStatus {
            message: Some("Saving Base...".into()),
            kind: SaveLoadStatusKind::Info,
            last_completed_id: None,
        };
        clear_loading_status_if_idle(&idle, &mut status);
        assert_eq!(status.message.as_deref(), Some("Saving Base..."));
    }

    /// Outstanding-scan admission state: a refresh requested against a
    /// missing directory, so the worker is present (the scan is
    /// outstanding) but inert and settles on the first polls.
    struct DeferredFixture {
        config: SaveLoadConfig,
        catalog: SaveCatalog,
        pending_scan: PendingCatalogScan,
        sim: SimResource,
        pending: PendingSaveJobs,
        confirmation: PendingSaveConfirmation,
        status: SaveLoadStatus,
        metrics: SaveLoadMetrics,
        deferred: DeferredNamedSave,
    }

    impl DeferredFixture {
        fn with_outstanding_scan() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "factory-deferred-save-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let config = SaveLoadConfig {
                root_dir: dir,
                autosave_interval_ticks: 300,
                autosave_slot_count: 5,
            };
            let catalog = SaveCatalog::default();
            let mut pending_scan = PendingCatalogScan::default();
            request_catalog_scan(&config, &mut pending_scan, catalog.scan_epoch);
            assert!(
                !pending_scan.is_empty(),
                "the refresh scan must be outstanding"
            );
            Self {
                config,
                catalog,
                pending_scan,
                sim: SimResource::new(factory_sim::Simulation::new_test_world(7)),
                pending: PendingSaveJobs::default(),
                confirmation: PendingSaveConfirmation::default(),
                status: SaveLoadStatus::default(),
                metrics: SaveLoadMetrics::default(),
                deferred: DeferredNamedSave::default(),
            }
        }

        fn guarded(&mut self, name: &str) -> bool {
            let Self {
                config,
                catalog,
                pending_scan,
                sim,
                pending,
                confirmation,
                status,
                metrics,
                deferred,
            } = self;
            request_named_save_guarded(
                name,
                sim,
                config,
                catalog,
                pending_scan,
                deferred,
                pending,
                confirmation,
                status,
                metrics,
            )
        }

        fn overwrite(&mut self, id: &SaveId) -> bool {
            let Self {
                sim,
                catalog,
                pending,
                status,
                metrics,
                deferred,
                ..
            } = self;
            request_overwrite(id, sim, catalog, pending, status, metrics, deferred)
        }

        fn settle_scan(&mut self) {
            let Self {
                config,
                catalog,
                pending_scan,
                status,
                deferred,
                ..
            } = self;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !pending_scan.is_empty() {
                poll_catalog_scan(config, pending_scan, catalog, status, deferred);
                assert!(
                    std::time::Instant::now() < deadline,
                    "catalog scan did not settle"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    fn named_entry(display_name: &str) -> SaveEntry {
        let id = SaveId::new(format!("external-{display_name}"));
        SaveEntry {
            id: id.clone(),
            metadata: SaveMetadata {
                schema_version: 1,
                id,
                display_name: display_name.into(),
                kind: SaveKind::Named,
                completed_at_unix_ms: 0,
                application_version: "test".into(),
                world_seed: None,
            },
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: PathBuf::from(format!("external-{display_name}.factsim")),
            inspected: None,
        }
    }

    #[test]
    fn named_save_parks_while_catalog_refresh_outstanding() {
        // Admission during a refresh must park instead of validating
        // uniqueness against the stale catalog; once the scan lands, the
        // parked request is admitted.
        let mut fixture = DeferredFixture::with_outstanding_scan();
        assert!(
            !fixture.guarded("Base"),
            "admission from a stale catalog must park instead of queueing"
        );
        assert_eq!(fixture.deferred.name.as_deref(), Some("Base"));
        assert_eq!(fixture.pending.queued_len(), 0);
        assert_eq!(fixture.status.kind, SaveLoadStatusKind::Info);
        fixture.settle_scan();
        let name = fixture.deferred.name.take().expect("parked request");
        assert!(fixture.guarded(&name));
        assert!(fixture.deferred.name.is_none());
        // Admission starts the worker immediately when idle, so the
        // request lives in running rather than the queue.
        assert!(
            !fixture.pending.is_empty(),
            "the settled request must be admitted"
        );
        assert_eq!(
            fixture.status.message.as_deref(),
            Some("Saving Base..."),
            "admission must report the requested save"
        );
    }

    #[test]
    fn known_name_routes_to_overwrite_despite_outstanding_scan() {
        // No duplicate can arise when the name is already listed: the
        // request routes to overwrite confirmation immediately instead of
        // parking, no matter what the outstanding scan later reports.
        let mut fixture = DeferredFixture::with_outstanding_scan();
        fixture.catalog.entries.push(named_entry("Base"));
        assert!(!fixture.guarded("base"));
        assert!(
            fixture.deferred.name.is_none(),
            "an overwrite routing must not park"
        );
        assert_eq!(fixture.pending.queued_len(), 0);
        assert!(
            matches!(fixture.confirmation, PendingSaveConfirmation::Overwrite(_)),
            "a listed name must ask for overwrite instead, got: {:?}",
            fixture.confirmation
        );
    }

    #[test]
    fn readmitting_name_expires_dropped_failure_context() {
        // Review follow-up: a dropped request whose name the user retries
        // must not haunt later failures. Retrying admits a new request
        // that supersedes the old context; a later scan failure must be
        // generic instead of falsely reporting the (meanwhile committed)
        // save as uncreated.
        let mut fixture = DeferredFixture::with_outstanding_scan();
        fixture.settle_scan();
        // Stale context from failures that have since settled.
        fixture.deferred.dropped_name = Some("Base".into());
        assert!(fixture.guarded("Base"));
        assert!(
            fixture.deferred.dropped_name.is_none(),
            "admitting the retried name must expire the dropped context"
        );
        assert!(
            !fixture.pending.is_empty(),
            "the retried request must be admitted"
        );
    }

    #[test]
    fn rejected_retry_preserves_dropped_failure_context() {
        // Review follow-up: the dropped context may only be cleared by
        // confirmed admission. A retry that request_named_save rejects —
        // the name is already saving, or the catalog routes it to
        // overwrite confirmation — accepts nothing, so the original save
        // stays uncreated and the context must survive.
        let mut fixture = DeferredFixture::with_outstanding_scan();
        fixture.settle_scan();
        assert!(fixture.guarded("Base"));
        // The first attempt is still in flight (nothing is ever polled
        // here), so retrying the name is rejected as already saving.
        fixture.deferred.dropped_name = Some("Base".into());
        assert!(!fixture.guarded("Base"));
        assert_eq!(
            fixture.status.message.as_deref(),
            Some("Base is already being saved.")
        );
        assert_eq!(
            fixture.deferred.dropped_name.as_deref(),
            Some("Base"),
            "a rejected retry must not discard the dropped context"
        );
        // Routed to overwrite confirmation: likewise no admission.
        fixture.catalog.entries.push(named_entry("Base"));
        assert!(!fixture.guarded("base"));
        assert!(matches!(
            fixture.confirmation,
            PendingSaveConfirmation::Overwrite(_)
        ));
        assert_eq!(
            fixture.deferred.dropped_name.as_deref(),
            Some("Base"),
            "an overwrite routing must not discard the dropped context"
        );
    }

    #[test]
    fn confirmed_overwrite_clears_dropped_failure_context() {
        // Review follow-up: overwrite routing preserves the dropped
        // context while the dialog is pending, but confirming queues a
        // replacement that supersedes it. A later scan failure must not
        // report the replaced save as uncreated.
        let mut fixture = DeferredFixture::with_outstanding_scan();
        fixture.settle_scan();
        fixture.catalog.entries.push(named_entry("Base"));
        let id = fixture.catalog.entries[0].id.clone();
        fixture.deferred.dropped_name = Some("Base".into());
        assert!(fixture.overwrite(&id));
        assert!(
            fixture.deferred.dropped_name.is_none(),
            "a confirmed overwrite must expire the dropped context"
        );
        assert!(
            !fixture.pending.is_empty(),
            "the replacement overwrite must be queued"
        );
        // A rejected overwrite leaves the context intact: another entry
        // normalizing to the same name is already saving under a
        // different id, so queueing refuses and nothing is accepted.
        fixture.deferred.dropped_name = Some("Base".into());
        fixture.catalog.entries.push(named_entry("BASE"));
        let other = fixture.catalog.entries[1].id.clone();
        assert!(!fixture.overwrite(&other));
        assert_eq!(
            fixture.status.message.as_deref(),
            Some("BASE is already being saved.")
        );
        assert_eq!(
            fixture.deferred.dropped_name.as_deref(),
            Some("Base"),
            "a rejected overwrite must not discard the dropped context"
        );
    }

    #[test]
    fn deferred_named_save_rechecks_uniqueness_after_scan_lands() {
        // Review scenario: the user creates "Base" while a refresh is
        // outstanding, and an external actor adds a valid "Base" before it
        // lands. Draining must re-run the uniqueness check against the
        // landed catalog and ask for overwrite instead of duplicating the
        // display name.
        let mut fixture = DeferredFixture::with_outstanding_scan();
        assert!(!fixture.guarded("Base"));
        assert_eq!(fixture.deferred.name.as_deref(), Some("Base"));
        fixture.settle_scan();
        // The external "Base" arrives with the landed refresh.
        fixture.catalog.entries.push(named_entry("Base"));
        let name = fixture.deferred.name.take().expect("parked request");
        assert!(!fixture.guarded(&name));
        assert!(
            fixture.deferred.name.is_none(),
            "a resolved request must not stay parked"
        );
        assert_eq!(
            fixture.pending.queued_len(),
            0,
            "the duplicate display name must not be admitted"
        );
        assert!(
            matches!(fixture.confirmation, PendingSaveConfirmation::Overwrite(_)),
            "the landed duplicate must ask for overwrite instead, got: {:?}",
            fixture.confirmation
        );
    }

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
                world_seed: None,
            },
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: PathBuf::from(format!("autosave-{generation}.factsim")),
            inspected: None,
        }
    }
}
