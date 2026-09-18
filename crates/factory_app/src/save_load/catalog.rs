use super::compatibility::classify_header;
use super::container::{
    CONTAINER_VERSION, ContainerError, SaveArtifactKind, discard_save_artifact, fallback_metadata,
    inspect_container, inspect_container_from_reader, load_simulation, load_simulation_from_reader,
    parse_save_artifact, promote_backup, read_inspection_bytes, retired_save_artifact_primary,
    with_save_artifact_lock,
};
use super::{
    CachedSaveValidation, CatalogValidationJob, CatalogValidationOutcome, CatalogValidationRequest,
    SaveCatalog, SaveCompatibility, SaveEntry, SaveFileFingerprint, SaveFileIdentity,
    SaveFileMetadataFingerprint, SaveId, SaveKind, SaveLoadConfig, SaveMetadata,
    local_datetime_from_unix_ms,
};
use bevy::log::warn;
use bevy::prelude::ResMut;
use factory_data::PrototypeCatalog;
use factory_sim::{
    SAVE_HEADER_SIZE, SaveLimits, SaveLoadError, inspect_save_header, load_from_bytes,
    prototype_hash,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_CATALOG_VALIDATION_JOBS: usize = 1;
/// Number of retries after the initial validation attempt. Attempts are
/// numbered from zero and only attempts below this bound schedule a retry,
/// so at most three validations (attempts 0, 1, 2) run per refresh cycle.
/// Transient failures (brief locks, mid-sync replacement) are retried;
/// anything still unstable afterwards waits for the next refresh or pending
/// rescan instead of spinning the worker.
const MAX_CATALOG_VALIDATION_RETRIES: u8 = 2;
/// How often entries stuck at `ValidationPending` with no queued or running
/// validation are re-observed.
const PENDING_RESCAN_INTERVAL: Duration = Duration::from_secs(2);

/// Replaces the in-memory catalog after a lightweight scan, then queues bounded
/// background payload validation for cache misses.
pub fn refresh_catalog(config: &SaveLoadConfig, catalog: &mut SaveCatalog) -> Result<(), String> {
    poll_catalog_validation_jobs_inner(catalog);
    let (mut entries, current_hash) = scan_catalog_unvalidated(config)?;
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
        prepare_entry_validation(
            entry,
            current_hash,
            &catalog.validation_cache,
            &mut requests,
        );
    }
    catalog.replace(entries);
    for request in requests {
        queue_catalog_validation(catalog, request);
    }
    start_catalog_validation_jobs(catalog);
    Ok(())
}

/// Recovers interrupted saves and returns all recognized canonical entries.
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
        let Some((id, kind, fallback_name)) = recognized_file(&path, config.autosave_slot_count)
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

#[derive(Debug)]
struct RecoveryTarget {
    id: SaveId,
    kind: SaveKind,
    backups: Vec<PathBuf>,
    temporaries: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryState {
    Valid,
    IntactButIncompatible,
    Corrupt,
}

enum RecoveryBackup {
    Candidate(Vec<u8>),
    Corrupt,
    Inaccessible(ContainerError),
}

struct FileInspection {
    metadata: Option<SaveMetadata>,
    compatibility: SaveCompatibility,
    safe_to_replace: bool,
    inspected: Option<SaveFileMetadataFingerprint>,
}

/// Best-effort recovery never prevents the ordinary catalog from listing saves.
fn recover_interrupted_saves(config: &SaveLoadConfig, current_hash: u64) {
    let directory = match fs::read_dir(&config.root_dir) {
        Ok(directory) => directory,
        Err(error) => {
            warn!("failed to scan save recovery files: {error}");
            return;
        }
    };
    let mut targets = BTreeMap::<PathBuf, RecoveryTarget>::new();
    for item in directory {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                warn!("failed to inspect a save recovery entry: {error}");
                continue;
            }
        };
        let artifact = item.path();
        if !artifact.is_file() {
            continue;
        }
        if retired_save_artifact_primary(&artifact).is_some() {
            remove_recovery_artifact(&artifact);
            continue;
        }
        let Some((primary, artifact_kind)) = parse_save_artifact(&artifact) else {
            continue;
        };
        let Some((id, kind, _)) = recognized_file(&primary, config.autosave_slot_count) else {
            continue;
        };
        let target = targets.entry(primary).or_insert_with(|| RecoveryTarget {
            id,
            kind,
            backups: Vec::new(),
            temporaries: Vec::new(),
        });
        match artifact_kind {
            SaveArtifactKind::Temporary => target.temporaries.push(artifact),
            SaveArtifactKind::Backup => target.backups.push(artifact),
        }
    }

    for (primary, target) in targets {
        for temporary in target.temporaries {
            remove_recovery_artifact(&temporary);
        }
        let primary_exists = match primary.try_exists() {
            Ok(exists) => exists,
            Err(error) => {
                warn!(
                    "cannot inspect save {} for recovery: {error}",
                    primary.display()
                );
                continue;
            }
        };
        let state = primary_exists.then(|| primary_state(&primary, &target.kind, current_hash));
        match state {
            Some(PrimaryState::Valid) => {
                for backup in target.backups {
                    remove_recovery_artifact(&backup);
                }
                continue;
            }
            Some(PrimaryState::IntactButIncompatible) => continue,
            Some(PrimaryState::Corrupt) | None => {}
        }

        let mut candidates: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        let mut validation_deferred = false;
        for backup in target.backups {
            match validate_recovery_backup(&backup, &target.id, &target.kind, current_hash) {
                RecoveryBackup::Candidate(bytes) => {
                    if candidates.iter().any(|(_, existing)| existing == &bytes) {
                        remove_recovery_artifact(&backup);
                    } else {
                        candidates.push((backup, bytes));
                    }
                }
                RecoveryBackup::Corrupt => remove_recovery_artifact(&backup),
                RecoveryBackup::Inaccessible(error) => {
                    validation_deferred = true;
                    warn!(
                        "cannot validate recovery backup {}: {error}",
                        backup.display()
                    );
                }
            }
        }
        if validation_deferred || candidates.len() != 1 {
            continue;
        }
        let (backup, _) = candidates.pop().expect("length checked");
        if let Err(error) = promote_backup(&backup, &primary, primary_exists) {
            warn!(
                "failed to recover save {} from {}: {error}",
                primary.display(),
                backup.display()
            );
        }
    }
}

/// Removes one stale artifact without aborting catalog availability on failure.
fn remove_recovery_artifact(path: &Path) {
    if let Err(error) = discard_save_artifact(path) {
        warn!(
            "failed to remove stale save artifact {}: {error}",
            path.display()
        );
    }
}

/// Fully classifies a primary only when lightweight inspection says it is compatible.
fn primary_state(path: &Path, kind: &SaveKind, current_hash: u64) -> PrimaryState {
    let inspection = inspect_file(path, kind, current_hash);
    match inspection.compatibility {
        SaveCompatibility::Compatible | SaveCompatibility::MigratableSaveFormat { .. } => {
            match load_simulation(path) {
                Ok(_) => PrimaryState::Valid,
                Err(ContainerError::Simulation(error)) => classify_simulation_result(Err(error)),
                Err(ContainerError::Io(_) | ContainerError::TooLarge) => {
                    PrimaryState::IntactButIncompatible
                }
                Err(_) => PrimaryState::Corrupt,
            }
        }
        SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
            if inspection.safe_to_replace {
                PrimaryState::Corrupt
            } else {
                PrimaryState::IntactButIncompatible
            }
        }
        _ => PrimaryState::IntactButIncompatible,
    }
}

fn classify_simulation_result(
    result: Result<factory_sim::Simulation, SaveLoadError>,
) -> PrimaryState {
    match result {
        Ok(_) => PrimaryState::Valid,
        Err(
            SaveLoadError::UnsupportedSaveVersion { .. }
            | SaveLoadError::UnsupportedPrototypeFormatVersion { .. }
            | SaveLoadError::TooLarge,
        ) => PrimaryState::IntactButIncompatible,
        Err(
            SaveLoadError::InvalidMagic { .. }
            | SaveLoadError::PrototypeHashMismatch { .. }
            | SaveLoadError::InvalidSimulationState(_)
            | SaveLoadError::Codec(_),
        ) => PrimaryState::Corrupt,
    }
}

/// Classifies a backup without deleting data that is merely incompatible or
/// temporarily unreadable. Candidate bytes are retained for duplicate checks.
fn validate_recovery_backup(
    path: &Path,
    id: &SaveId,
    kind: &SaveKind,
    current_hash: u64,
) -> RecoveryBackup {
    let bytes = match super::container::read_save_artifact(path) {
        Ok(bytes) => bytes,
        // A bounded read cannot establish corruption. Retain oversized backups
        // too: they may be intact saves from a build with a different policy.
        Err(error) => return RecoveryBackup::Inaccessible(error),
    };

    if !bytes.starts_with(&super::container::CONTAINER_MAGIC) {
        if kind != &SaveKind::Quicksave {
            return RecoveryBackup::Corrupt;
        }
        return match classify_inspection(&bytes, current_hash) {
            SaveCompatibility::Compatible | SaveCompatibility::MigratableSaveFormat { .. } => {
                let result = load_from_bytes(&bytes);
                classify_backup_result(bytes, result)
            }
            SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
                RecoveryBackup::Corrupt
            }
            _ => RecoveryBackup::Candidate(bytes),
        };
    }

    let container = match inspect_container(path) {
        Ok(container) => container,
        Err(error @ ContainerError::Io(_)) => return RecoveryBackup::Inaccessible(error),
        Err(_) => return RecoveryBackup::Corrupt,
    };
    if container
        .metadata
        .as_ref()
        .is_some_and(|metadata| &metadata.id != id || &metadata.kind != kind)
    {
        return RecoveryBackup::Corrupt;
    }
    if container.version != CONTAINER_VERSION {
        return RecoveryBackup::Candidate(bytes);
    }

    match classify_inspection(&container.simulation_header, current_hash) {
        SaveCompatibility::Compatible | SaveCompatibility::MigratableSaveFormat { .. } => {
            match super::container::container_payload_offset(&bytes) {
                Ok(offset) => {
                    let result = load_from_bytes(&bytes[offset..]);
                    classify_backup_result(bytes, result)
                }
                Err(error @ ContainerError::Io(_)) => RecoveryBackup::Inaccessible(error),
                Err(_) => RecoveryBackup::Corrupt,
            }
        }
        SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
            RecoveryBackup::Corrupt
        }
        _ => RecoveryBackup::Candidate(bytes),
    }
}

fn classify_backup_result(
    bytes: Vec<u8>,
    result: Result<factory_sim::Simulation, SaveLoadError>,
) -> RecoveryBackup {
    match result {
        Ok(_) => RecoveryBackup::Candidate(bytes),
        Err(SaveLoadError::TooLarge) => RecoveryBackup::Inaccessible(ContainerError::TooLarge),
        Err(_) => RecoveryBackup::Corrupt,
    }
}

/// Maps canonical file names to stable save identities and kinds.
fn recognized_file(path: &Path, autosave_count: usize) -> Option<(SaveId, SaveKind, String)> {
    let file_name = path.file_name()?.to_str()?;
    if file_name == "quicksave.factsim" {
        return Some((
            SaveId::new("quicksave"),
            SaveKind::Quicksave,
            "Quicksave".into(),
        ));
    }
    if let Some(number) = file_name
        .strip_prefix("autosave-")
        .and_then(|value| value.strip_suffix(".factsim"))
        .and_then(|value| value.parse::<usize>().ok())
    {
        if (1..=autosave_count).contains(&number) {
            return Some((
                SaveId::new(format!("autosave-{number}")),
                SaveKind::Autosave { generation: number },
                format!("Autosave {number}"),
            ));
        }
        return None;
    }
    let opaque = file_name
        .strip_prefix("manual-")?
        .strip_suffix(".factsim")?;
    if opaque.is_empty()
        || !opaque
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return None;
    }
    let id = SaveId::new(format!("manual-{opaque}"));
    Some((id, SaveKind::Named, format!("Named Save {opaque}")))
}

/// Builds one catalog entry using the shared lightweight file inspection.
fn inspect_entry(
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

fn prepare_entry_validation(
    entry: &mut SaveEntry,
    current_hash: u64,
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
    let Ok(mut file) = fs::File::open(&entry.path) else {
        // The file is momentarily unreadable. Publish the pending state
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
    let metadata = save_file_metadata_fingerprint(&file);
    if entry.compatibility == SaveCompatibility::ValidationPending
        || entry
            .inspected
            .as_ref()
            .is_some_and(|inspected| inspected != &metadata)
    {
        // No classification exists for these bytes, or the path was replaced
        // after header inspection. Re-derive it from the instance this
        // handle observes before pairing it with an identity.
        entry.compatibility = classify_open_save(&mut file, &entry.metadata.kind, current_hash);
        entry.inspected = Some(metadata.clone());
        if !entry.compatibility.can_load()
            && entry.compatibility != SaveCompatibility::ValidationPending
        {
            return;
        }
    }
    let source_compatibility = entry.compatibility.clone();
    if metadata.len > SaveLimits::default().max_encoded_bytes {
        entry.compatibility = SaveCompatibility::ExceedsCurrentLimits;
    } else if metadata.identity.is_some()
        && let Some(cached) = cache.get(&entry.path)
        && cached.fingerprint.metadata == metadata
    {
        entry.compatibility = cached.compatibility.clone();
    } else {
        entry.compatibility = SaveCompatibility::ValidationPending;
        requests.push(CatalogValidationRequest {
            path: entry.path.clone(),
            kind: entry.metadata.kind.clone(),
            compatibility: source_compatibility,
            metadata,
            attempt: 0,
        });
    }
}

fn queue_catalog_validation(catalog: &mut SaveCatalog, request: CatalogValidationRequest) {
    if catalog
        .validation_jobs
        .iter()
        .any(|job| job.path == request.path && job.metadata == request.metadata)
    {
        return;
    }
    if catalog
        .validation_queue
        .iter()
        .any(|queued| queued.path == request.path && queued.metadata == request.metadata)
    {
        return;
    }
    catalog
        .validation_queue
        .retain(|queued| queued.path != request.path);
    catalog.validation_queue.push_back(request);
}

/// Builds a validation request for a file that could not be opened for
/// fingerprinting. The worker classifies from its own handle on arrival, so
/// the placeholder compatibility and identity below are never published.
fn blind_retry_request(path: PathBuf, kind: SaveKind, attempt: u8) -> CatalogValidationRequest {
    CatalogValidationRequest {
        path,
        kind,
        compatibility: SaveCompatibility::ValidationPending,
        metadata: SaveFileMetadataFingerprint {
            len: 0,
            modified: None,
            identity: None,
        },
        attempt,
    }
}

/// Opens the file currently on disk and returns its identity together with the
/// header compatibility derived from that same handle. Retries queue this
/// pair so a classification is never bound to a different file instance.
fn inspect_current_file(
    path: &Path,
    kind: &SaveKind,
) -> Option<(SaveFileMetadataFingerprint, SaveCompatibility)> {
    let mut file = fs::File::open(path).ok()?;
    let metadata = save_file_metadata_fingerprint(&file);
    if metadata.len > SaveLimits::default().max_encoded_bytes {
        return Some((metadata, SaveCompatibility::ExceedsCurrentLimits));
    }
    let current_hash = prototype_hash(&PrototypeCatalog::load_base().ok()?);
    let compatibility = classify_open_save(&mut file, kind, current_hash);
    Some((metadata, compatibility))
}

fn start_catalog_validation_jobs(catalog: &mut SaveCatalog) {
    while catalog.validation_jobs.len() < MAX_CATALOG_VALIDATION_JOBS {
        let Some(request) = catalog.validation_queue.pop_front() else {
            break;
        };
        let path = request.path.clone();
        let metadata = request.metadata.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let handle = thread::spawn(move || {
            validate_loadable_path(
                request.path,
                request.kind,
                request.compatibility,
                Some(request.metadata),
                request.attempt,
                worker_cancel,
            )
        });
        catalog.validation_jobs.push(CatalogValidationJob {
            path,
            metadata,
            cancel,
            handle,
        });
    }
}

/// Wraps a save handle so catalog shutdown can abort hashing and decoding
/// at the next read chunk instead of waiting for the remainder.
struct CancelReader<R> {
    reader: R,
    cancel: Arc<AtomicBool>,
}

impl<R: Read> Read for CancelReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "catalog validation cancelled",
            ));
        }
        self.reader.read(buffer)
    }
}

impl<R: Seek> Seek for CancelReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.reader.seek(position)
    }
}

pub(crate) fn poll_catalog_validation_jobs(mut catalog: ResMut<SaveCatalog>) {
    poll_catalog_validation_jobs_inner(&mut catalog);
}

fn poll_catalog_validation_jobs_inner(catalog: &mut SaveCatalog) {
    let mut index = 0;
    while index < catalog.validation_jobs.len() {
        if !catalog.validation_jobs[index].handle.is_finished() {
            index += 1;
            continue;
        }
        let job = catalog.validation_jobs.swap_remove(index);
        let path = job.path.clone();
        let outcome = match job.handle.join() {
            Ok(outcome) => outcome,
            Err(_) => {
                // The entry stays pending; the rescan timer re-observes it.
                warn!(
                    "catalog validation worker for {} panicked; retrying",
                    path.display()
                );
                continue;
            }
        };
        let current_metadata = fs::File::open(&outcome.path)
            .ok()
            .map(|file| save_file_metadata_fingerprint(&file));
        let still_current = match (&outcome.fingerprint, &outcome.observed_metadata) {
            (Some(fingerprint), _) => current_metadata.as_ref() == Some(&fingerprint.metadata),
            (None, Some(observed))
                if outcome.compatibility == SaveCompatibility::ExceedsCurrentLimits =>
            {
                current_metadata.as_ref() == Some(observed)
            }
            (None, None) => false,
            (None, Some(_)) => false,
        };
        if still_current && outcome.compatibility != SaveCompatibility::ValidationPending {
            if let Some(fingerprint) = outcome.fingerprint {
                catalog.validation_cache.insert(
                    outcome.path.clone(),
                    CachedSaveValidation {
                        fingerprint,
                        compatibility: outcome.compatibility.clone(),
                    },
                );
            }
            if let Some(entry) = catalog
                .entries
                .iter_mut()
                .find(|entry| entry.path == outcome.path)
                && entry.compatibility == SaveCompatibility::ValidationPending
            {
                entry.compatibility = outcome.compatibility;
                catalog.revision = catalog.revision.wrapping_add(1);
            }
        } else if outcome.attempt < MAX_CATALOG_VALIDATION_RETRIES
            && let Some(entry_index) = catalog
                .entries
                .iter()
                .position(|entry| entry.path == outcome.path)
            && catalog.entries[entry_index].compatibility == SaveCompatibility::ValidationPending
            && !catalog
                .validation_queue
                .iter()
                .any(|queued| queued.path == outcome.path)
        {
            let kind = catalog.entries[entry_index].metadata.kind.clone();
            let next_attempt = outcome.attempt + 1;
            if let Some((metadata, fresh)) = inspect_current_file(&outcome.path, &kind) {
                if fresh.can_load() {
                    queue_catalog_validation(
                        catalog,
                        CatalogValidationRequest {
                            path: outcome.path,
                            kind,
                            compatibility: fresh,
                            metadata,
                            attempt: next_attempt,
                        },
                    );
                } else {
                    catalog.entries[entry_index].compatibility = fresh;
                    catalog.revision = catalog.revision.wrapping_add(1);
                }
            } else {
                // The file is momentarily unreadable (brief lock, mid-sync
                // replacement). Queue a bounded blind retry.
                queue_catalog_validation(
                    catalog,
                    blind_retry_request(outcome.path, kind, next_attempt),
                );
            }
        } else if outcome.attempt >= MAX_CATALOG_VALIDATION_RETRIES
            && !catalog
                .validation_queue
                .iter()
                .any(|queued| queued.path == outcome.path)
            && !catalog
                .validation_jobs
                .iter()
                .any(|job| job.path == outcome.path)
            && let Some(entry) = catalog
                .entries
                .iter_mut()
                .find(|entry| entry.path == outcome.path)
            && entry.compatibility == SaveCompatibility::ValidationPending
        {
            // Retries are exhausted: reconcile the display with the file
            // currently on disk instead of leaving a stale pending row.
            // Loadable candidates stay pending for the periodic rescan.
            let kind = entry.metadata.kind.clone();
            if let Some((_, fresh)) = inspect_current_file(&outcome.path, &kind)
                && !fresh.can_load()
            {
                entry.compatibility = fresh;
                catalog.revision = catalog.revision.wrapping_add(1);
            }
        }
    }
    rescan_stale_pending(catalog);
    start_catalog_validation_jobs(catalog);
}

/// Re-observes entries stuck at `ValidationPending` with no queued or
/// running validation, at most every `PENDING_RESCAN_INTERVAL_MS`. This
/// covers attempt exhaustion and refresh-time open failures, so recovered
/// access is picked up without waiting for an unrelated catalog refresh.
/// Each rescan performs a single observation per entry; a still-failing
/// entry simply waits for the next interval.
fn rescan_stale_pending(catalog: &mut SaveCatalog) {
    let now = Instant::now();
    if catalog.next_pending_rescan.is_some_and(|due| now < due) {
        return;
    }
    catalog.next_pending_rescan = Some(now + PENDING_RESCAN_INTERVAL);
    let stale: Vec<(PathBuf, SaveKind)> = catalog
        .entries
        .iter()
        .filter(|entry| entry.compatibility == SaveCompatibility::ValidationPending)
        .filter(|entry| {
            !catalog
                .validation_queue
                .iter()
                .any(|queued| queued.path == entry.path)
                && !catalog
                    .validation_jobs
                    .iter()
                    .any(|job| job.path == entry.path)
        })
        .map(|entry| (entry.path.clone(), entry.metadata.kind.clone()))
        .collect();
    for (path, kind) in stale {
        // Observe once: a still-unreadable file fails without chaining, and
        // the next interval tries again.
        queue_catalog_validation(
            catalog,
            blind_retry_request(path, kind, MAX_CATALOG_VALIDATION_RETRIES),
        );
    }
}

/// Fully validates a loadable payload synchronously for explicit callers such
/// as recovery tests. Interactive catalog refreshes use the background path.
/// The header is always classified from the same open handle that is
/// fingerprinted below, so a replacement between catalog inspection and this
/// call cannot pair a stale classification with the current bytes.
fn validate_loadable_file(
    path: &Path,
    kind: &SaveKind,
    current_hash: u64,
    cache: &mut BTreeMap<PathBuf, CachedSaveValidation>,
) -> SaveCompatibility {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return SaveCompatibility::ValidationPending,
    };
    let metadata = save_file_metadata_fingerprint(&file);
    if metadata.len > SaveLimits::default().max_encoded_bytes {
        return SaveCompatibility::ExceedsCurrentLimits;
    }
    let header_compatibility = classify_open_save(&mut file, kind, current_hash);
    if !header_compatibility.can_load() {
        return header_compatibility;
    }
    if metadata.identity.is_some()
        && let Some(cached) = cache.get(path)
        && cached.fingerprint.metadata == metadata
    {
        return cached.compatibility.clone();
    }
    let outcome = validate_loadable_path(
        path.to_path_buf(),
        kind.clone(),
        header_compatibility,
        Some(metadata),
        0,
        Arc::new(AtomicBool::new(false)),
    );
    if let Some(fingerprint) = outcome.fingerprint {
        cache.insert(
            path.to_path_buf(),
            CachedSaveValidation {
                fingerprint,
                compatibility: outcome.compatibility.clone(),
            },
        );
    }
    outcome.compatibility
}

fn validate_loadable_path(
    path: PathBuf,
    kind: SaveKind,
    source_compatibility: SaveCompatibility,
    expected: Option<SaveFileMetadataFingerprint>,
    attempt: u8,
    cancel: Arc<AtomicBool>,
) -> CatalogValidationOutcome {
    let mut file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(_) => {
            return CatalogValidationOutcome {
                path,
                compatibility: SaveCompatibility::ValidationPending,
                observed_metadata: None,
                fingerprint: None,
                attempt,
            };
        }
    };
    let metadata = save_file_metadata_fingerprint(&file);
    if metadata.len > SaveLimits::default().max_encoded_bytes {
        return CatalogValidationOutcome {
            path,
            compatibility: SaveCompatibility::ExceedsCurrentLimits,
            observed_metadata: Some(metadata),
            fingerprint: None,
            attempt,
        };
    }
    // The path may have been replaced after the request was created. Only a
    // classification derived from this handle may be published for it. A
    // carried `ValidationPending` is a blind-retry placeholder, never a
    // classification, so it is always re-derived.
    let source_compatibility = match expected {
        Some(expected)
            if expected == metadata
                && source_compatibility != SaveCompatibility::ValidationPending =>
        {
            source_compatibility
        }
        _ => match PrototypeCatalog::load_base()
            .ok()
            .map(|catalog| prototype_hash(&catalog))
        {
            Some(current_hash) => classify_open_save(&mut file, &kind, current_hash),
            None => {
                return CatalogValidationOutcome {
                    path,
                    compatibility: SaveCompatibility::ValidationPending,
                    observed_metadata: Some(metadata),
                    fingerprint: None,
                    attempt,
                };
            }
        },
    };
    if !source_compatibility.can_load() {
        return CatalogValidationOutcome {
            path,
            compatibility: source_compatibility,
            observed_metadata: Some(metadata),
            fingerprint: None,
            attempt,
        };
    }
    // From here on every byte is read through the cancellation wrapper so
    // catalog shutdown aborts hashing and decoding at the next read chunk.
    let mut file = CancelReader {
        reader: file,
        cancel,
    };
    let fingerprint = match save_file_fingerprint(&mut file, metadata.clone()) {
        Ok(fingerprint) => fingerprint,
        Err(ContainerError::TooLarge) => {
            return CatalogValidationOutcome {
                path,
                compatibility: SaveCompatibility::ExceedsCurrentLimits,
                observed_metadata: Some(metadata),
                fingerprint: None,
                attempt,
            };
        }
        Err(_) => {
            return CatalogValidationOutcome {
                path,
                compatibility: SaveCompatibility::ValidationPending,
                observed_metadata: Some(metadata),
                fingerprint: None,
                attempt,
            };
        }
    };
    let load_result = match file.rewind() {
        Ok(()) => {
            load_simulation_from_reader(&mut BufReader::new(&mut file), SaveLimits::default())
        }
        Err(error) => Err(ContainerError::Io(error)),
    };
    let compatibility = match load_result {
        Ok(_) => source_compatibility.clone(),
        Err(ContainerError::TooLarge) => SaveCompatibility::ExceedsCurrentLimits,
        Err(ContainerError::Io(_)) => SaveCompatibility::ValidationPending,
        Err(_) => SaveCompatibility::CorruptOrTruncated,
    };
    let after_metadata = save_file_metadata_fingerprint(&file.reader);
    let stable_fingerprint = match save_file_fingerprint(&mut file, after_metadata.clone()) {
        Ok(after_validation) if after_validation == fingerprint => Some(fingerprint),
        _ => None,
    };
    CatalogValidationOutcome {
        path,
        compatibility,
        observed_metadata: Some(after_metadata),
        fingerprint: stable_fingerprint,
        attempt,
    }
}

/// Identifies the exact bytes considered by payload validation.
fn save_file_fingerprint(
    file: &mut (impl Read + Seek),
    metadata: SaveFileMetadataFingerprint,
) -> Result<SaveFileFingerprint, ContainerError> {
    file.rewind()?;
    let maximum = SaveLimits::default().max_encoded_bytes;
    if metadata.len > maximum {
        return Err(ContainerError::TooLarge);
    }
    let mut hasher = blake3::Hasher::new();
    let copied = io::copy(
        &mut Read::by_ref(file).take(maximum.saturating_add(1)),
        &mut hasher,
    )?;
    if copied > maximum {
        return Err(ContainerError::TooLarge);
    }
    Ok(SaveFileFingerprint {
        metadata: SaveFileMetadataFingerprint {
            len: copied,
            ..metadata
        },
        content_digest: *hasher.finalize().as_bytes(),
    })
}

fn save_file_metadata_fingerprint(file: &fs::File) -> SaveFileMetadataFingerprint {
    match file.metadata() {
        Ok(metadata) => SaveFileMetadataFingerprint {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            identity: save_file_identity(file, &metadata),
        },
        Err(_) => SaveFileMetadataFingerprint {
            len: 0,
            modified: None,
            identity: None,
        },
    }
}

#[cfg(unix)]
fn save_file_identity(_file: &fs::File, metadata: &fs::Metadata) -> Option<SaveFileIdentity> {
    use std::os::unix::fs::MetadataExt;

    let mut file_id = [0; 16];
    file_id[..8].copy_from_slice(&metadata.ino().to_le_bytes());
    Some(SaveFileIdentity {
        volume_or_device: metadata.dev(),
        file_id,
        change_time: metadata.ctime(),
        change_time_nanoseconds: metadata.ctime_nsec(),
    })
}

#[cfg(windows)]
fn save_file_identity(file: &fs::File, _metadata: &fs::Metadata) -> Option<SaveFileIdentity> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FILE_ID_INFO, FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx,
    };

    let mut id = FILE_ID_INFO::default();
    let mut basic = FILE_BASIC_INFO::default();
    // SAFETY: both output pointers refer to correctly sized writable structs,
    // and the borrowed file keeps the handle valid for both calls.
    let id_ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&raw mut id).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } != 0;
    // SAFETY: same argument validity as above, with FILE_BASIC_INFO.
    let basic_ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw mut basic).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } != 0;
    (id_ok && basic_ok).then_some(SaveFileIdentity {
        volume_or_device: id.VolumeSerialNumber,
        file_id: id.FileId.Identifier,
        change_time: basic.ChangeTime,
        change_time_nanoseconds: 0,
    })
}

#[cfg(not(any(unix, windows)))]
fn save_file_identity(_file: &fs::File, _metadata: &fs::Metadata) -> Option<SaveFileIdentity> {
    None
}

/// Performs the shared lightweight container/header classification used by
/// both catalog display and full recovery safety checks.
///
/// Reads that never observe file bytes (open and other I/O failures)
/// report `ValidationPending` instead of corruption so a transient denial
/// is retried rather than mislabelled; only observed malformation or
/// truncation reports `CorruptOrTruncated`.
fn inspect_file(path: &Path, kind: &SaveKind, current_hash: u64) -> FileInspection {
    match fs::File::open(path) {
        Ok(mut file) => inspect_open_file(&mut file, kind, current_hash),
        Err(_) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::ValidationPending,
            safe_to_replace: false,
            inspected: None,
        },
    }
}

/// Classifies the header visible through an already-open save handle and
/// records that handle's identity, binding the classification to the file
/// instance instead of the path.
fn inspect_open_file(file: &mut fs::File, kind: &SaveKind, current_hash: u64) -> FileInspection {
    let inspected = Some(save_file_metadata_fingerprint(file));
    let complete = |metadata, compatibility: SaveCompatibility| {
        let safe_to_replace = matches!(
            compatibility,
            SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave
        );
        FileInspection {
            metadata,
            compatibility,
            safe_to_replace,
            inspected: inspected.clone(),
        }
    };
    if file.rewind().is_err() {
        return FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::ValidationPending,
            safe_to_replace: false,
            inspected,
        };
    }
    match inspect_container_from_reader(file) {
        Ok(container) => {
            let compatibility = if container.version != CONTAINER_VERSION {
                SaveCompatibility::UnsupportedContainerVersion {
                    found: container.version,
                    supported: CONTAINER_VERSION,
                }
            } else {
                classify_inspection(&container.simulation_header, current_hash)
            };
            complete(container.metadata, compatibility)
        }
        Err(ContainerError::InvalidContainerMagic) if kind == &SaveKind::Quicksave => {
            let mut header = vec![0; SAVE_HEADER_SIZE];
            let read = file
                .rewind()
                .map_err(ContainerError::Io)
                .and_then(|()| read_inspection_bytes(file, &mut header));
            match read {
                Ok(()) => complete(None, classify_inspection(&header, current_hash)),
                // An established short file is corrupt; any other failure
                // observed no bytes and stays pending for a retry.
                Err(ContainerError::Truncated) => FileInspection {
                    metadata: None,
                    compatibility: SaveCompatibility::CorruptOrTruncated,
                    safe_to_replace: false,
                    inspected,
                },
                Err(_) => FileInspection {
                    metadata: None,
                    compatibility: SaveCompatibility::ValidationPending,
                    safe_to_replace: false,
                    inspected,
                },
            }
        }
        Err(ContainerError::InvalidContainerMagic) => {
            complete(None, SaveCompatibility::NotFactorySave)
        }
        Err(ContainerError::Io(_)) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::ValidationPending,
            safe_to_replace: false,
            inspected,
        },
        Err(_) => complete(None, SaveCompatibility::CorruptOrTruncated),
    }
}

/// Re-derives header compatibility from an already-open handle. Callers that
/// detected a path replacement use this so a stale classification is never
/// paired with a new file instance.
fn classify_open_save(
    file: &mut fs::File,
    kind: &SaveKind,
    current_hash: u64,
) -> SaveCompatibility {
    inspect_open_file(file, kind, current_hash).compatibility
}

/// Maps a simulation header parse into user-facing compatibility.
fn classify_inspection(header: &[u8], current_hash: u64) -> SaveCompatibility {
    match inspect_save_header(header) {
        Ok(header) => classify_header(header, current_hash),
        Err(SaveLoadError::InvalidMagic { .. }) => SaveCompatibility::NotFactorySave,
        Err(_) => SaveCompatibility::CorruptOrTruncated,
    }
}

/// Returns a best-effort file modification timestamp for fallback metadata.
fn file_timestamp_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|timestamp| local_datetime_from_unix_ms(*timestamp).is_some())
        .unwrap_or(0)
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

/// Returns the current wall-clock timestamp used in save metadata.
pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_refresh_defers_payload_validation_to_a_bounded_worker() {
        let root = std::env::temp_dir().join(format!(
            "factory-background-validation-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("quicksave.factsim"),
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim"),
        )
        .unwrap();
        let config = SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let mut catalog = SaveCatalog::default();

        refresh_catalog(&config, &mut catalog).unwrap();

        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::ValidationPending
        );
        assert_eq!(catalog.validation_jobs.len(), 1);
        assert!(catalog.validation_queue.is_empty());
        drop(catalog);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stable_file_identity_invalidates_same_size_timestamp_cache_entry() {
        let path = std::env::temp_dir().join(format!(
            "factory-migration-cache-{}-{}.factsim",
            std::process::id(),
            now_unix_ms()
        ));
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let mut file = fs::File::open(&path).unwrap();
        let metadata = save_file_metadata_fingerprint(&file);
        let current_fingerprint = save_file_fingerprint(&mut file, metadata).unwrap();
        let mut stale_fingerprint = current_fingerprint.clone();
        stale_fingerprint
            .metadata
            .identity
            .as_mut()
            .expect("test filesystem should expose stable file identity")
            .change_time ^= 1;
        let mut cache = BTreeMap::from([(
            path.clone(),
            CachedSaveValidation {
                fingerprint: stale_fingerprint,
                compatibility: SaveCompatibility::CorruptOrTruncated,
            },
        )]);
        let header_compatibility = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let compatibility =
            validate_loadable_file(&path, &SaveKind::Quicksave, current_hash, &mut cache);

        assert_eq!(compatibility, header_compatibility);
        assert_eq!(cache[&path].fingerprint, current_fingerprint);
        assert_eq!(cache[&path].compatibility, header_compatibility);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn oversized_migration_candidate_is_rejected_before_hashing() {
        let path = std::env::temp_dir().join(format!(
            "factory-oversized-migration-{}-{}.factsim",
            std::process::id(),
            now_unix_ms()
        ));
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(SaveLimits::default().max_encoded_bytes + 1)
            .unwrap();
        drop(file);
        let mut cache = BTreeMap::new();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let compatibility =
            validate_loadable_file(&path, &SaveKind::Quicksave, current_hash, &mut cache);

        assert_eq!(compatibility, SaveCompatibility::ExceedsCurrentLimits);
        assert!(cache.is_empty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn policy_rejection_preserves_primary_and_defers_backup_recovery() {
        let sim = factory_sim::Simulation::new_test_world(292);
        let bytes = factory_sim::save_to_bytes(&sim).unwrap();
        let limits = factory_sim::SaveLimits {
            max_decoded_bytes: 1,
            ..Default::default()
        };
        let rejected = || factory_sim::load_from_bytes_with_limits(&bytes, limits);
        assert_eq!(
            classify_simulation_result(rejected()),
            PrimaryState::IntactButIncompatible
        );
        assert!(matches!(
            classify_backup_result(bytes.clone(), rejected()),
            RecoveryBackup::Inaccessible(ContainerError::TooLarge)
        ));
        assert_eq!(
            classify_simulation_result(load_from_bytes(&bytes)),
            PrimaryState::Valid
        );
        assert!(matches!(
            classify_backup_result(bytes.clone(), load_from_bytes(&bytes)),
            RecoveryBackup::Candidate(_)
        ));
        assert_eq!(
            classify_simulation_result(load_from_bytes(&[0])),
            PrimaryState::Corrupt
        );
        assert!(matches!(
            classify_backup_result(vec![0], load_from_bytes(&[0])),
            RecoveryBackup::Corrupt
        ));
    }

    #[test]
    fn stale_retry_preserves_queued_current_header_classification() {
        use std::collections::VecDeque;

        let dir = std::env::temp_dir().join(format!(
            "factory-stale-retry-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let mut open_a = fs::File::open(&path).unwrap();
        let metadata_a = save_file_metadata_fingerprint(&open_a);
        let fingerprint_a = save_file_fingerprint(&mut open_a, metadata_a.clone()).unwrap();
        drop(open_a);

        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata_b = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_ne!(
            metadata_a, metadata_b,
            "replacement should change file identity for the test"
        );
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());
        assert_eq!(
            inspect_file(&path, &SaveKind::Quicksave, current_hash).compatibility,
            SaveCompatibility::Compatible,
            "replacement bytes should classify as current format"
        );

        let migratable = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };
        let stale = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: migratable.clone(),
            observed_metadata: Some(metadata_a.clone()),
            fingerprint: Some(fingerprint_a),
            attempt: 0,
        };
        // Simulate the old worker finishing after the replacement + refresh.
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: crate::save_load::SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    crate::save_load::SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: VecDeque::from([CatalogValidationRequest {
                path: path.clone(),
                kind: SaveKind::Quicksave,
                compatibility: SaveCompatibility::Compatible,
                metadata: metadata_b.clone(),
                attempt: 0,
            }]),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: metadata_a,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| stale),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible,
            "stale v57 retry must not publish Migratable for the replaced v58 file"
        );
        assert_eq!(
            catalog
                .validation_cache
                .get(&path)
                .map(|cached| cached.compatibility.clone()),
            Some(SaveCompatibility::Compatible)
        );
        assert_eq!(
            catalog.validation_cache[&path].fingerprint.metadata,
            metadata_b
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_retry_recomputes_current_header_without_queued_request() {
        let dir = std::env::temp_dir().join(format!(
            "factory-stale-retry-fresh-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let mut open_a = fs::File::open(&path).unwrap();
        let metadata_a = save_file_metadata_fingerprint(&open_a);
        let fingerprint_a = save_file_fingerprint(&mut open_a, metadata_a.clone()).unwrap();
        drop(open_a);

        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata_b = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());

        let migratable = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };
        let stale = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: migratable,
            observed_metadata: Some(metadata_a.clone()),
            fingerprint: Some(fingerprint_a),
            attempt: 0,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: crate::save_load::SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    crate::save_load::SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: metadata_a,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| stale),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible,
            "retry after replacement must use the current header, not the stale source"
        );
        assert_eq!(
            catalog.validation_cache[&path].fingerprint.metadata,
            metadata_b
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replaced_file_between_inspection_and_request_uses_current_header() {
        let dir = std::env::temp_dir().join(format!(
            "factory-inspection-replacement-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        // Header inspection observes the v57 file.
        let mut entry = inspect_entry(
            path.clone(),
            SaveId::new("quicksave"),
            SaveKind::Quicksave,
            "Quicksave".into(),
            current_hash,
        );
        assert!(matches!(
            entry.compatibility,
            SaveCompatibility::MigratableSaveFormat { .. }
        ));

        // The path is replaced by a current-format save before the validation
        // request is created.
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();

        let cache = BTreeMap::new();
        let mut requests = Vec::new();
        prepare_entry_validation(&mut entry, current_hash, &cache, &mut requests);

        assert_eq!(entry.compatibility, SaveCompatibility::ValidationPending);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].compatibility, SaveCompatibility::Compatible);
        assert_eq!(requests[0].kind, SaveKind::Quicksave);
        let current_metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_eq!(requests[0].metadata, current_metadata);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn worker_replacement_after_request_uses_current_header() {
        let dir = std::env::temp_dir().join(format!(
            "factory-worker-replacement-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        let migratable = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };

        // The request carries a current-format classification, but the path
        // now resolves to v57 bytes.
        fs::write(&path, &current).unwrap();
        let metadata_current = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        fs::write(&path, historical).unwrap();
        let outcome = validate_loadable_path(
            path.clone(),
            SaveKind::Quicksave,
            SaveCompatibility::Compatible,
            Some(metadata_current),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(outcome.compatibility, migratable);
        let validated_metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_eq!(
            outcome.fingerprint.map(|fingerprint| fingerprint.metadata),
            Some(validated_metadata)
        );

        // And the reverse: a stale migratable classification must not be
        // published for current bytes.
        let metadata_historical = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        fs::write(&path, &current).unwrap();
        let outcome = validate_loadable_path(
            path.clone(),
            SaveKind::Quicksave,
            migratable,
            Some(metadata_historical),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(outcome.compatibility, SaveCompatibility::Compatible);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn second_transient_failure_schedules_bounded_retry() {
        let dir = std::env::temp_dir().join(format!(
            "factory-transient-retry-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());

        // The first validation and its retry both hit transient I/O
        // failures. Loading must not stay disabled.
        let transient = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::ValidationPending,
            observed_metadata: Some(metadata),
            fingerprint: None,
            attempt: 1,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: save_file_metadata_fingerprint(&fs::File::open(&path).unwrap()),
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| transient),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_file_schedules_bounded_blind_retry() {
        let dir = std::env::temp_dir().join(format!(
            "factory-blind-retry-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata_before = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        fs::remove_file(&path).unwrap();

        let transient = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::ValidationPending,
            observed_metadata: Some(metadata_before.clone()),
            fingerprint: None,
            attempt: 0,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: metadata_before,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| transient),
            }],
        };

        // Wait until the blind retry is in flight: it carries a placeholder
        // identity because the file could not be opened for fingerprinting.
        let placeholder = SaveFileMetadataFingerprint {
            len: 0,
            modified: None,
            identity: None,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while catalog.validation_jobs.len() != 1
            || catalog.validation_jobs[0].metadata != placeholder
        {
            assert!(
                std::time::Instant::now() < deadline,
                "blind retry was not scheduled"
            );
            poll_catalog_validation_jobs_inner(&mut catalog);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // The file reappears while the blind retry is outstanding. Either the
        // running retry or its bounded successor must observe and publish it.
        fs::write(&path, &current).unwrap();
        drain_validation_jobs(&mut catalog);

        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        assert!(catalog.validation_cache[&path].fingerprint.metadata.len > 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retries_stop_at_attempt_bound() {
        let dir = std::env::temp_dir().join(format!(
            "factory-retry-bound-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        // The file is valid and present, but the attempt budget is spent:
        // no further validation may be scheduled by the chain. The periodic
        // rescan (disabled here) owns any later observation.
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        let transient = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::ValidationPending,
            observed_metadata: Some(metadata),
            fingerprint: None,
            attempt: MAX_CATALOG_VALIDATION_RETRIES,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: SaveFileMetadataFingerprint {
                    len: 0,
                    modified: None,
                    identity: None,
                },
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| transient),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::ValidationPending
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepare_open_failure_queues_blind_retry() {
        let dir = std::env::temp_dir().join(format!(
            "factory-prepare-blind-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        // The file is momentarily unreadable: no bytes were ever observed.
        let path = dir.join("quicksave.factsim");
        let mut entry = SaveEntry {
            id: SaveId::new("quicksave"),
            metadata: fallback_metadata(
                SaveId::new("quicksave"),
                SaveKind::Quicksave,
                "Quicksave".into(),
                0,
            ),
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: path.clone(),
            inspected: None,
        };
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let cache = BTreeMap::new();
        let mut requests = Vec::new();
        prepare_entry_validation(&mut entry, current_hash, &cache, &mut requests);

        assert_eq!(entry.compatibility, SaveCompatibility::ValidationPending);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attempt, 0);
        assert_eq!(
            requests[0].compatibility,
            SaveCompatibility::ValidationPending
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exhausted_pending_entry_is_rescanned() {
        let dir = std::env::temp_dir().join(format!(
            "factory-exhausted-rescan-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();

        // Retries are exhausted and no validation is scheduled, but the file
        // is valid: the periodic rescan must observe and publish it.
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: None,
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: Vec::new(),
        };

        // The drain helper only polls while jobs exist; prime one poll so
        // the rescan (which is what schedules work here) can run.
        poll_catalog_validation_jobs_inner(&mut catalog);
        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        assert!(catalog.next_pending_rescan.is_some());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancel_reader_aborts_reads_when_set() {
        let bytes = b"FACTSIM payload";
        let cancel = Arc::new(AtomicBool::new(false));
        let mut reader = CancelReader {
            reader: std::io::Cursor::new(&bytes[..]),
            cancel: cancel.clone(),
        };
        let mut first = [0; 7];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"FACTSIM");
        cancel.store(true, Ordering::Relaxed);
        assert!(reader.read(&mut first).is_err());
    }

    #[test]
    fn cancelled_decode_aborts_without_consuming_payload() {
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        let cancel = Arc::new(AtomicBool::new(true));
        let mut reader = CancelReader {
            reader: std::io::Cursor::new(&current),
            cancel,
        };
        let result = load_simulation_from_reader(&mut reader, factory_sim::SaveLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn drop_signals_running_jobs_before_joining() {
        let dir = std::env::temp_dir().join(format!(
            "factory-drop-cancel-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let catalog = SaveCatalog {
            entries: Vec::new(),
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: SaveFileMetadataFingerprint {
                    len: 0,
                    modified: None,
                    identity: None,
                },
                cancel,
                handle: thread::spawn(move || {
                    let outcome = CatalogValidationOutcome {
                        path,
                        compatibility: SaveCompatibility::ValidationPending,
                        observed_metadata: None,
                        fingerprint: None,
                        attempt: 0,
                    };
                    let mut spins = 0u32;
                    loop {
                        if worker_cancel.load(Ordering::Relaxed) {
                            let _ = done_tx.send(true);
                            return outcome;
                        }
                        spins += 1;
                        if spins > 2_000 {
                            let _ = done_tx.send(false);
                            return outcome;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }),
            }],
        };

        drop(catalog);

        // Fails (after ~2s) if drop joins without signalling first.
        assert_eq!(
            done_rx.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(true)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn inspection_io_failure_is_pending_not_corrupt() {
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());
        let missing = std::env::temp_dir().join(format!(
            "factory-inspect-missing-{}-{}.factsim",
            std::process::id(),
            now_unix_ms()
        ));
        let _ = fs::remove_file(&missing);

        let inspection = inspect_file(&missing, &SaveKind::Quicksave, current_hash);
        assert_eq!(
            inspection.compatibility,
            SaveCompatibility::ValidationPending
        );
        assert!(!inspection.safe_to_replace);
        assert!(inspection.inspected.is_none());
    }

    #[test]
    fn inspection_short_file_is_corrupt() {
        let dir = std::env::temp_dir().join(format!(
            "factory-inspect-short-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        fs::write(&path, b"short").unwrap();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let inspection = inspect_file(&path, &SaveKind::Quicksave, current_hash);
        assert_eq!(
            inspection.compatibility,
            SaveCompatibility::CorruptOrTruncated
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepare_pending_entry_with_readable_file_queues_validation() {
        let dir = std::env::temp_dir().join(format!(
            "factory-prepare-pending-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        // Inspection failed transiently, so no classification exists, but the
        // file is readable now.
        let mut entry = SaveEntry {
            id: SaveId::new("quicksave"),
            metadata: fallback_metadata(
                SaveId::new("quicksave"),
                SaveKind::Quicksave,
                "Quicksave".into(),
                0,
            ),
            compatibility: SaveCompatibility::ValidationPending,
            metadata_available: true,
            path: path.clone(),
            inspected: None,
        };
        let cache = BTreeMap::new();
        let mut requests = Vec::new();
        prepare_entry_validation(&mut entry, current_hash, &cache, &mut requests);

        assert_eq!(entry.compatibility, SaveCompatibility::ValidationPending);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].compatibility, SaveCompatibility::Compatible);
        fs::remove_dir_all(dir).unwrap();
    }

    fn drain_validation_jobs(catalog: &mut SaveCatalog) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !catalog.validation_jobs.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "validation job did not finish"
            );
            poll_catalog_validation_jobs_inner(catalog);
            if !catalog.validation_jobs.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn old_and_unrelated_file_names_are_ignored() {
        let count = 5;
        for name in [
            "slot_1.factsim",
            "slot_2.factsim",
            "slot_3.factsim",
            "autosave.factsim",
            "quicksave.factsim.tmp-1",
            "file.txt",
        ] {
            assert!(recognized_file(Path::new(name), count).is_none());
        }
        assert!(recognized_file(Path::new("manual-abc.factsim"), count).is_some());
        assert!(recognized_file(Path::new("autosave-5.factsim"), count).is_some());
    }
}
