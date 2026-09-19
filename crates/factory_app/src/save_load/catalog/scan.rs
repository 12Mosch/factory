//! Directory scanning: refresh the in-memory catalog from disk.
//!
//! Scanning is lightweight: entries are built from [`super::inspect`]
//! header classification, then [`super::validation`] fills in payload
//! verdicts in the background. [`super::recovery`] runs first so
//! interrupted writes are settled before anything is listed.

use super::super::container::{
    fallback_metadata, try_with_save_artifact_lock, with_save_artifact_lock,
};
use super::super::{
    CachedSaveValidation, CatalogValidationRequest, SaveCatalog, SaveCompatibility, SaveEntry,
    SaveId, SaveKind, SaveLoadConfig, local_datetime_from_unix_ms,
};
use super::inspect::{
    classify_open_save, file_timestamp_ms, inspect_file, save_file_metadata_fingerprint,
};
use super::polling::poll_catalog_validation_jobs_inner;
use super::recovery::recover_interrupted_saves;
use super::validation::{
    blind_retry_request, queue_catalog_validation, start_catalog_validation_jobs,
    validate_loadable_file,
};
use bevy::log::warn;
use factory_data::PrototypeCatalog;
use factory_sim::{SaveLimits, prototype_hash};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Replaces the in-memory catalog after a lightweight scan, then queues bounded
/// background payload validation for cache misses. Recovery is best-effort and
/// non-blocking: when the background writer holds the artifact lock, recovery
/// is deferred to the next refresh instead of stalling the frame.
pub fn refresh_catalog(config: &SaveLoadConfig, catalog: &mut SaveCatalog) -> Result<(), String> {
    refresh_catalog_inner(config, catalog, false)
}

/// Startup variant: recovery blocks on the artifact lock so the first frame
/// observes a settled catalog. At real startup no writer exists yet, so this
/// does not stall the UI; under parallel tests it waits out other tests'
/// writers instead of deferring cleanup nobody re-triggers.
pub fn refresh_catalog_blocking(
    config: &SaveLoadConfig,
    catalog: &mut SaveCatalog,
) -> Result<(), String> {
    refresh_catalog_inner(config, catalog, true)
}

fn refresh_catalog_inner(
    config: &SaveLoadConfig,
    catalog: &mut SaveCatalog,
    blocking_recovery: bool,
) -> Result<(), String> {
    poll_catalog_validation_jobs_inner(catalog);
    let (mut entries, current_hash) = scan_catalog_unvalidated(config, blocking_recovery)?;
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
/// Explicit callers (tests, one-shot scans) block on the artifact lock so
/// recovery is deterministic; interactive refreshes use the non-blocking path.
pub fn scan_catalog(config: &SaveLoadConfig) -> Result<Vec<SaveEntry>, String> {
    let (mut entries, current_hash) = scan_catalog_unvalidated(config, true)?;
    let mut cache = BTreeMap::new();
    for entry in &mut entries {
        if entry.compatibility.can_load() {
            entry.compatibility =
                validate_loadable_file(&entry.path, &entry.metadata.kind, current_hash, &mut cache);
        }
    }
    Ok(entries)
}

fn scan_catalog_unvalidated(
    config: &SaveLoadConfig,
    blocking: bool,
) -> Result<(Vec<SaveEntry>, u64), String> {
    if !config.root_dir.exists() {
        return Ok((Vec::new(), 0));
    }
    let current_hash = prototype_hash(
        &PrototypeCatalog::load_base()
            .map_err(|error| format!("failed to load prototype data: {error}"))?,
    );
    if blocking {
        with_save_artifact_lock(|| recover_interrupted_saves(config, current_hash));
    } else if try_with_save_artifact_lock(|| recover_interrupted_saves(config, current_hash))
        .is_none()
    {
        // Never block the frame on the background writer: recovery mutates
        // artifacts under the same lock held during encoding and commit. When
        // the writer is busy, skip recovery this refresh and retry on the next
        // poll; directory listing and header inspection below stay lock-free
        // and bounded, and payload validation already runs on workers.
        warn!("save catalog recovery deferred: background save holds the artifact lock");
    }
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

pub(crate) fn prepare_entry_validation(
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
