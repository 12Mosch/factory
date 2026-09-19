//! Directory scanning: refresh the in-memory catalog from disk.
//!
//! All filesystem work — recovery, directory listing, and header
//! inspection — runs on a single background scan worker. Frame schedules
//! only request scans and install finished results, so catalog refreshes
//! never block the UI on the writer lock or on slow filesystems. Payload
//! verdicts still arrive via [`super::validation`] workers afterwards.

use super::super::container::{fallback_metadata, with_save_artifact_lock};
use super::super::{
    CachedSaveValidation, CatalogValidationRequest, DeferredNamedSave, SaveCatalog,
    SaveCompatibility, SaveEntry, SaveId, SaveKind, SaveLoadConfig, SaveLoadStatus,
    SaveLoadStatusKind, local_datetime_from_unix_ms,
};
use super::inspect::{file_timestamp_ms, inspect_file};
use super::recovery::recover_interrupted_saves;
use super::validation::{
    blind_retry_request, queue_catalog_validation, start_catalog_validation_jobs,
    validate_loadable_file,
};
use bevy::log::warn;
use bevy::prelude::{Res, ResMut, Resource};
use factory_data::PrototypeCatalog;
use factory_sim::{SaveLimits, prototype_hash};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

/// At most one background catalog scan runs at a time. Frame schedules only
/// request scans and install finished results; the worker owns every file
/// open, read, stat, and recovery mutation.
#[derive(Resource, Default)]
pub struct PendingCatalogScan {
    worker: Option<RunningCatalogScan>,
    requested: bool,
    /// Catalog mutation epoch for a requested follow-up scan. The running
    /// worker keeps its own epoch (see [`RunningCatalogScan`]): a newer
    /// request must never relabel an in-flight scan, or a stale snapshot
    /// could pass the freshness check and resurrect removed entries.
    requested_epoch: u64,
}

/// One in-flight scan worker with the catalog mutation epoch it observed at
/// spawn. Compared against the live epoch only when this specific worker
/// lands, so a later request cannot re-tag it as current.
struct RunningCatalogScan {
    epoch: u64,
    handle: JoinHandle<Result<Vec<SaveEntry>, String>>,
}

impl PendingCatalogScan {
    /// Whether no scan is running or requested.
    pub fn is_empty(&self) -> bool {
        self.worker.is_none() && !self.requested
    }

    fn join_finished(&mut self) -> Option<(u64, Result<Vec<SaveEntry>, String>)> {
        let finished = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.handle.is_finished());
        if !finished {
            return None;
        }
        let worker = self.worker.take().expect("worker checked");
        let outcome = worker
            .handle
            .join()
            .unwrap_or_else(|_| Err("catalog scan worker panicked".into()));
        Some((worker.epoch, outcome))
    }
}

impl Drop for PendingCatalogScan {
    fn drop(&mut self) {
        // A finished scan is joined so a worker panic still surfaces its
        // payload. A still-running worker is detached instead of stalling
        // teardown on a held artifact lock or a slow filesystem: at
        // shutdown no consumer remains for its snapshot, and recovery
        // mutations are atomic renames designed to survive interruption —
        // abandoning them equals a crash mid-recovery, which the next
        // startup's recovery handles.
        if let Some(worker) = self.worker.take()
            && worker.handle.is_finished()
        {
            let _ = worker.handle.join();
        }
    }
}

/// Requests a background catalog scan. Coalesces while a scan is running —
/// a follow-up scan starts when the running one lands if anything requested
/// one meanwhile. The running worker keeps the epoch it started with; a
/// newer request records only the follow-up epoch and never relabels the
/// in-flight scan, so a scan overtaken by a mutation is dropped on landing
/// instead of resurrecting removed entries.
pub(crate) fn request_catalog_scan(
    config: &SaveLoadConfig,
    pending: &mut PendingCatalogScan,
    epoch: u64,
) {
    if pending.worker.is_some() {
        pending.requested = true;
        pending.requested_epoch = epoch;
        return;
    }
    let root_dir = config.root_dir.clone();
    let slot_count = config.autosave_slot_count;
    pending.requested = false;
    pending.requested_epoch = epoch;
    pending.worker = Some(RunningCatalogScan {
        epoch,
        handle: thread::spawn(move || {
            scan_catalog_unvalidated(&SaveLoadConfig {
                root_dir,
                autosave_interval_ticks: 0,
                autosave_slot_count: slot_count,
            })
            .map(|(entries, _)| entries)
        }),
    });
}

/// Installs a finished background scan and queues bounded background payload
/// validation for cache misses. Purely in-memory: no file opens, reads, or
/// stats on the frame schedule.
pub(crate) fn poll_catalog_scan(
    config: &SaveLoadConfig,
    pending: &mut PendingCatalogScan,
    catalog: &mut SaveCatalog,
    status: &mut SaveLoadStatus,
    deferred: &mut DeferredNamedSave,
) {
    let Some((worker_epoch, outcome)) = pending.join_finished() else {
        return;
    };
    let mut follow_up = pending.requested;
    match outcome {
        Ok(entries) if worker_epoch == catalog.scan_epoch => {
            install_scan_entries(catalog, entries);
            // Freshness re-established: the dropped-request context
            // expires with the stale catalog it referred to.
            deferred.dropped_name = None;
        }
        Ok(_) => {
            // A catalog mutation (e.g. a deletion) landed while this
            // specific worker was in flight: the snapshot predates it and
            // may resurrect removed entries, so it is dropped unseen and a
            // follow-up scan re-observes the directory.
            follow_up = true;
        }
        Err(error) => {
            status.kind = SaveLoadStatusKind::Error;
            status.last_completed_id = None;
            if let Some(name) = deferred.name.take() {
                // Freshness was never established: the catalog is still
                // stale, so the parked request must not drain against it
                // once the (now empty) scan state settles. Fail it
                // explicitly — naming the save and preserving the refresh
                // failure — instead of minting a possible duplicate. One
                // follow-up scan is requested so a manual retry usually
                // meets a fresh catalog. The dropped name is retained: a
                // follow-up that also fails re-reports it below instead of
                // replacing it with a generic error.
                deferred.dropped_name = Some(name.clone());
                status.message = Some(format!(
                    "Cannot refresh save catalog: {error}; {name} was not created."
                ));
                follow_up = true;
            } else if let Some(name) = &deferred.dropped_name {
                // A follow-up (or later) scan failed while a dropped
                // request is still unacknowledged-by-freshness: preserve
                // its failure instead of disconnecting the settled status
                // from the lost user action. No further follow-up: a
                // persistent failure must settle, not spin.
                status.message = Some(format!(
                    "Cannot refresh save catalog: {error}; {name} was not created."
                ));
            } else {
                status.message = Some(format!("Cannot refresh save catalog: {error}"));
            }
        }
    }
    if follow_up {
        request_catalog_scan(config, pending, catalog.scan_epoch);
    }
}

pub(crate) fn poll_catalog_scan_system(
    config: Res<SaveLoadConfig>,
    mut pending: ResMut<PendingCatalogScan>,
    mut catalog: ResMut<SaveCatalog>,
    mut status: ResMut<SaveLoadStatus>,
    mut deferred: ResMut<DeferredNamedSave>,
) {
    poll_catalog_scan(
        &config,
        &mut pending,
        &mut catalog,
        &mut status,
        &mut deferred,
    );
}

fn install_scan_entries(catalog: &mut SaveCatalog, mut entries: Vec<SaveEntry>) {
    let present = entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    catalog
        .validation_cache
        .retain(|path, _| present.contains(path));
    catalog
        .validation_queue
        .retain(|request| present.contains(&request.path));
    let mut requests = Vec::new();
    for entry in &mut entries {
        plan_entry_validation(entry, &catalog.validation_cache, &mut requests);
    }
    catalog.replace(entries);
    for request in requests {
        queue_catalog_validation(catalog, request);
    }
    start_catalog_validation_jobs(catalog);
}

/// Startup and screen-transition variant: scans synchronously with blocking
/// recovery so the first frame observes a settled catalog. At real startup
/// no writer exists yet, so this does not stall the UI.
pub fn refresh_catalog_blocking(
    config: &SaveLoadConfig,
    catalog: &mut SaveCatalog,
) -> Result<(), String> {
    let (entries, _) = scan_catalog_unvalidated(config)?;
    install_scan_entries(catalog, entries);
    Ok(())
}

/// Recovers interrupted saves and returns all recognized canonical entries.
/// Recovery always takes the artifact lock blocking-style; interactive
/// refreshes run it on the background scan worker (see
/// [`request_catalog_scan`]), so frames never wait on it. Shutdown joins a
/// finished scan worker and detaches a still-running one instead of
/// stalling teardown — see [`PendingCatalogScan`].
pub fn scan_catalog(config: &SaveLoadConfig) -> Result<Vec<SaveEntry>, String> {
    let (mut entries, current_hash) = scan_catalog_unvalidated(config)?;
    let mut cache = BTreeMap::new();
    for entry in &mut entries {
        if entry.compatibility.can_load() {
            entry.compatibility =
                validate_loadable_file(&entry.path, &entry.metadata.kind, current_hash, &mut cache);
        }
    }
    Ok(entries)
}

fn scan_catalog_unvalidated(config: &SaveLoadConfig) -> Result<(Vec<SaveEntry>, u64), String> {
    if !config.root_dir.exists() {
        return Ok((Vec::new(), 0));
    }
    let current_hash = prototype_hash(
        &PrototypeCatalog::load_base()
            .map_err(|error| format!("failed to load prototype data: {error}"))?,
    );
    // Blocking recovery is safe here: this only runs on background scan
    // workers and in explicit synchronous scans, never on frame schedules.
    with_save_artifact_lock(|| recover_interrupted_saves(config, current_hash));
    let directory = fs::read_dir(&config.root_dir)
        .map_err(|error| format!("failed to scan save directory: {error}"))?;
    let mut entries = Vec::new();
    for item in directory {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                warn!("failed to inspect a save-directory entry: {error}");
                continue;
            }
        };
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some((id, kind, fallback_name)) =
            super::inspect::recognized_file(&path, config.autosave_slot_count)
        else {
            continue;
        };
        entries.push(inspect_entry(path, id, kind, fallback_name, current_hash));
    }
    entries.sort_by(|left, right| {
        group_order(&left.metadata.kind)
            .cmp(&group_order(&right.metadata.kind))
            .then_with(|| {
                right
                    .metadata
                    .completed_at_unix_ms
                    .cmp(&left.metadata.completed_at_unix_ms)
            })
            .then_with(|| {
                autosave_generation(&left.metadata.kind)
                    .cmp(&autosave_generation(&right.metadata.kind))
            })
    });
    Ok((entries, current_hash))
}

/// Builds one catalog entry using the shared lightweight file inspection.
pub(crate) fn inspect_entry(
    path: PathBuf,
    id: SaveId,
    kind: SaveKind,
    fallback_name: String,
    current_hash: u64,
) -> SaveEntry {
    let timestamp = file_timestamp_ms(&path);
    let fallback = || fallback_metadata(id.clone(), kind.clone(), fallback_name.clone(), timestamp);
    let inspection = inspect_file(&path, &kind, current_hash);
    let metadata = inspection
        .metadata
        .filter(|metadata| metadata.id == id && metadata.kind == kind);
    let metadata_available = metadata.as_ref().is_some_and(|metadata| {
        local_datetime_from_unix_ms(metadata.completed_at_unix_ms).is_some()
    });
    let metadata = metadata
        .map(|mut metadata| {
            if !metadata_available {
                metadata.completed_at_unix_ms = timestamp;
            }
            metadata
        })
        .unwrap_or_else(fallback);
    SaveEntry {
        id,
        metadata,
        compatibility: inspection.compatibility,
        metadata_available,
        path,
        inspected: inspection.inspected,
    }
}

/// Plans background validation for one worker-scanned entry without touching
/// the filesystem. The scan worker already classified the entry from the
/// single handle it observed, so the frame only compares the observed
/// identity against the cache: hits apply immediately, misses decode on
/// validation workers, and never-inspected files get a blind retry.
pub(crate) fn plan_entry_validation(
    entry: &mut SaveEntry,
    cache: &BTreeMap<PathBuf, CachedSaveValidation>,
    requests: &mut Vec<CatalogValidationRequest>,
) {
    // Settled rejections need no work. A pending entry was never classified
    // (transient inspection I/O), so it must still be observed below.
    if !entry.compatibility.can_load()
        && entry.compatibility != SaveCompatibility::ValidationPending
    {
        return;
    }
    let Some(observed) = entry.inspected.clone() else {
        // The scan worker could not open the file. Publish the pending state
        // together with a blind retry so recovered access is observed
        // without waiting for an unrelated refresh.
        entry.compatibility = SaveCompatibility::ValidationPending;
        requests.push(blind_retry_request(
            entry.path.clone(),
            entry.metadata.kind.clone(),
            0,
        ));
        return;
    };
    let source_compatibility = entry.compatibility.clone();
    if observed.len > SaveLimits::default().max_encoded_bytes {
        entry.compatibility = SaveCompatibility::ExceedsCurrentLimits;
    } else if observed.identity.is_some()
        && let Some(cached) = cache.get(&entry.path)
        && cached.fingerprint.metadata == observed
    {
        entry.compatibility = cached.compatibility.clone();
    } else {
        entry.compatibility = SaveCompatibility::ValidationPending;
        requests.push(CatalogValidationRequest {
            path: entry.path.clone(),
            kind: entry.metadata.kind.clone(),
            compatibility: source_compatibility,
            metadata: observed,
            attempt: 0,
        });
    }
}

/// Assigns stable catalog groups for sorting.
fn group_order(kind: &SaveKind) -> u8 {
    match kind {
        SaveKind::Named => 0,
        SaveKind::Quicksave => 1,
        SaveKind::Autosave { .. } => 2,
    }
}

/// Extracts an autosave generation for deterministic tie-breaking.
fn autosave_generation(kind: &SaveKind) -> usize {
    match kind {
        SaveKind::Autosave { generation } => *generation,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_landing_after_delete_is_dropped_unseen() {
        let dir = std::env::temp_dir().join(format!(
            "factory-scan-epoch-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        // The directory does not exist, so the follow-up scan lands
        // immediately with nothing to install.
        let config = SaveLoadConfig {
            root_dir: dir.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let id = SaveId::new("deleted");
        let metadata = fallback_metadata(id.clone(), SaveKind::Named, "Deleted".into(), 0);
        let path = dir.join("deleted.factsim");
        let stale = SaveEntry {
            id: id.clone(),
            metadata: metadata.clone(),
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: path.clone(),
            inspected: None,
        };
        let mut catalog = SaveCatalog::default();
        catalog.entries.push(SaveEntry {
            id: id.clone(),
            metadata,
            compatibility: SaveCompatibility::ValidationPending,
            metadata_available: true,
            path,
            inspected: None,
        });
        catalog.scan_epoch = 0;
        let mut pending = PendingCatalogScan::default();
        let mut status = SaveLoadStatus::default();
        let mut deferred = DeferredNamedSave::default();
        // A scan requested before the deletion lands after it.
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(|| Ok(vec![stale])),
        });
        pending.requested = false;
        pending.requested_epoch = 0;
        catalog.remove(&id);
        // The stale snapshot must never become visible, including while
        // the follow-up scan is still in flight.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !pending.is_empty() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                catalog.entries.is_empty(),
                "a scan overtaken by a deletion resurrected the entry"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "catalog scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(catalog.entries.is_empty());
    }

    #[test]
    fn newer_request_never_relabels_running_scan() {
        // Scan A starts at epoch 0; a deletion bumps to 1; a second request
        // arrives while A is still running. A must still land as epoch 0
        // (dropped unseen) instead of being relabeled to 1 and accepted.
        let dir = std::env::temp_dir().join(format!(
            "factory-scan-relabel-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let config = SaveLoadConfig {
            root_dir: dir.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let id = SaveId::new("deleted");
        let metadata = fallback_metadata(id.clone(), SaveKind::Named, "Deleted".into(), 0);
        let path = dir.join("deleted.factsim");
        let stale = SaveEntry {
            id: id.clone(),
            metadata: metadata.clone(),
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: path.clone(),
            inspected: None,
        };
        let mut catalog = SaveCatalog::default();
        catalog.entries.push(SaveEntry {
            id: id.clone(),
            metadata,
            compatibility: SaveCompatibility::ValidationPending,
            metadata_available: true,
            path,
            inspected: None,
        });
        catalog.scan_epoch = 0;
        let mut pending = PendingCatalogScan::default();
        // Scan A starts at epoch 0 and blocks until released.
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(move || {
                let _ = release_rx.recv();
                Ok(vec![stale])
            }),
        });
        pending.requested = false;
        pending.requested_epoch = 0;
        // Deletion bumps the epoch, then a second scan request arrives
        // while A is still running: it must record only the follow-up.
        catalog.remove(&id);
        assert_eq!(catalog.scan_epoch, 1);
        request_catalog_scan(&config, &mut pending, catalog.scan_epoch);
        assert!(
            pending.requested,
            "a scan arriving during a running worker must queue a follow-up"
        );
        assert_eq!(
            pending.worker.as_ref().map(|worker| worker.epoch),
            Some(0),
            "the running scan must keep its original epoch"
        );
        // Release A: its stale snapshot lands as epoch 0 vs live epoch 1,
        // so it is dropped unseen and a follow-up re-observes.
        drop(release_tx);
        let mut status = SaveLoadStatus::default();
        let mut deferred = DeferredNamedSave::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !pending.is_empty() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                catalog.entries.is_empty(),
                "a relabeled stale scan resurrected the deleted entry"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "catalog scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(catalog.entries.is_empty());
    }

    #[test]
    fn failed_scan_fails_parked_named_save_explicitly() {
        // Review scenario: a named-save request parks while a refresh is
        // outstanding, then the refresh fails (e.g. read_dir errors). The
        // park must be failed explicitly — naming the save and preserving
        // the refresh failure — instead of draining against the still-stale
        // catalog once the empty scan state settles.
        let dir = std::env::temp_dir().join(format!(
            "factory-scan-error-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let config = SaveLoadConfig {
            root_dir: dir,
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let mut catalog = SaveCatalog::default();
        let mut pending = PendingCatalogScan::default();
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(|| Err("read_dir failed".into())),
        });
        let mut deferred = DeferredNamedSave {
            name: Some("Base".into()),
            dropped_name: None,
        };
        let mut status = SaveLoadStatus::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // Poll until the failing worker is joined and the error branch
        // runs; earlier polls no-op while it is still in flight.
        while deferred.name.is_some() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                std::time::Instant::now() < deadline,
                "catalog scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(status.kind, SaveLoadStatusKind::Error);
        assert_eq!(
            status.message.as_deref(),
            Some("Cannot refresh save catalog: read_dir failed; Base was not created."),
            "the failure must name the dropped save and preserve the refresh error, got: {status:?}"
        );
        assert!(
            !pending.is_empty(),
            "one follow-up scan must be requested so a manual retry meets a fresh catalog"
        );
        // The follow-up scans a missing directory and settles empty: the
        // failure must not chain into a retry loop.
        while !pending.is_empty() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                std::time::Instant::now() < deadline,
                "follow-up scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(pending.is_empty());
        assert!(deferred.name.is_none());
        assert!(
            deferred.dropped_name.is_none(),
            "the successful follow-up re-establishes freshness and expires the dropped context"
        );
    }

    #[test]
    fn consecutive_scan_failures_preserve_dropped_save_failure() {
        // Review follow-up: the explicit failure above is followed by one
        // bounded retry. If the filesystem error persists, that follow-up
        // reaches the generic error branch with nothing parked — it must
        // re-report the dropped save instead of replacing the message,
        // and must not chain another retry.
        let dir = std::env::temp_dir().join(format!(
            "factory-scan-error-repeat-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let config = SaveLoadConfig {
            root_dir: dir,
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let mut catalog = SaveCatalog::default();
        let mut pending = PendingCatalogScan::default();
        let mut deferred = DeferredNamedSave {
            name: Some("Base".into()),
            dropped_name: None,
        };
        let mut status = SaveLoadStatus::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        // First failure drops the park and requests one follow-up.
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(|| Err("permission denied".into())),
        });
        while deferred.name.is_some() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                std::time::Instant::now() < deadline,
                "catalog scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            status.message.as_deref(),
            Some("Cannot refresh save catalog: permission denied; Base was not created.")
        );
        // Replace the auto-requested follow-up with a second failure.
        assert!(
            !pending.is_empty(),
            "the first failure must request one follow-up"
        );
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(|| Err("permission denied".into())),
        });
        while pending.worker.is_some() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                std::time::Instant::now() < deadline,
                "follow-up scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            status.message.as_deref(),
            Some("Cannot refresh save catalog: permission denied; Base was not created."),
            "the follow-up failure must preserve the dropped save failure, got: {status:?}"
        );
        assert_eq!(status.kind, SaveLoadStatusKind::Error);
        assert!(
            pending.is_empty(),
            "a persistent failure must settle, not spin"
        );
        assert!(deferred.name.is_none());
        // A later successful scan expires the retained context.
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(|| Ok(Vec::new())),
        });
        while pending.worker.is_some() {
            poll_catalog_scan(
                &config,
                &mut pending,
                &mut catalog,
                &mut status,
                &mut deferred,
            );
            assert!(
                std::time::Instant::now() < deadline,
                "recovery scan did not settle"
            );
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(pending.is_empty());
        assert!(
            deferred.dropped_name.is_none(),
            "freshness re-established by a successful scan expires the context"
        );
    }

    #[test]
    fn drop_detaches_unfinished_scan_without_waiting() {
        use std::time::{Duration, Instant};
        // The worker blocks until the test releases it, simulating a scan
        // stuck on a held artifact lock or a slow filesystem at shutdown.
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let mut pending = PendingCatalogScan::default();
        pending.worker = Some(RunningCatalogScan {
            epoch: 0,
            handle: thread::spawn(move || {
                let _ = release_rx.recv();
                Ok(Vec::new())
            }),
        });
        let start = Instant::now();
        drop(pending);
        // Teardown must not stall on the unfinished worker.
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "shutdown join waited for an unfinished scan worker"
        );
        // Release the detached worker so it can exit on its own.
        drop(release_tx);
    }
}
