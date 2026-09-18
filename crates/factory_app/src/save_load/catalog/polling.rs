//! Bevy polling: collect finished validation workers, schedule retries,
//! and re-observe stale pending entries.
//!
//! The inner poll returns whether externally relevant catalog state
//! changed; pure timer bookkeeping does not count, so an idle catalog never
//! trips Bevy change detection.

use super::super::{
    CachedSaveValidation, CatalogValidationRequest, SaveCatalog, SaveCompatibility, SaveKind,
};
use super::inspect::{inspect_current_file, save_file_metadata_fingerprint};
use super::validation::{
    MAX_CATALOG_VALIDATION_RETRIES, blind_retry_request, queue_catalog_validation,
    start_catalog_validation_jobs,
};
use bevy::log::warn;
use bevy::prelude::{DetectChangesMut, ResMut};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How often entries stuck at `ValidationPending` with no queued or running
/// validation are re-observed.
const PENDING_RESCAN_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) fn poll_catalog_validation_jobs(mut catalog: ResMut<SaveCatalog>) {
    if poll_catalog_validation_jobs_inner(catalog.bypass_change_detection()) {
        catalog.set_changed();
    }
}

/// Collects finished validation jobs and schedules retries/rescans.
/// Returns whether externally relevant catalog state (entries, cache,
/// revision, queues, or jobs) changed; pure timer bookkeeping does not
/// count, so an idle catalog never trips Bevy change detection.
pub(crate) fn poll_catalog_validation_jobs_inner(catalog: &mut SaveCatalog) -> bool {
    let mut changed = false;
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
                changed = true;
            }
            if let Some(entry) = catalog
                .entries
                .iter_mut()
                .find(|entry| entry.path == outcome.path)
                && entry.compatibility == SaveCompatibility::ValidationPending
            {
                entry.compatibility = outcome.compatibility;
                catalog.revision = catalog.revision.wrapping_add(1);
                changed = true;
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
                    changed |= queue_catalog_validation(
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
                    changed = true;
                }
            } else {
                // The file is momentarily unreadable (brief lock, mid-sync
                // replacement). Queue a bounded blind retry.
                changed |= queue_catalog_validation(
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
                changed = true;
            }
        }
    }
    changed |= rescan_stale_pending(catalog);
    changed |= start_catalog_validation_jobs(catalog);
    changed
}

/// Re-observes entries stuck at `ValidationPending` with no queued or
/// running validation, at most every `PENDING_RESCAN_INTERVAL`. This
/// covers attempt exhaustion and refresh-time open failures, so recovered
/// access is picked up without waiting for an unrelated catalog refresh.
/// Each rescan performs a single observation per entry; a still-failing
/// entry simply waits for the next interval. Returns whether any request
/// was queued; advancing the throttle alone does not count.
fn rescan_stale_pending(catalog: &mut SaveCatalog) -> bool {
    let now = Instant::now();
    if catalog.next_pending_rescan.is_some_and(|due| now < due) {
        return false;
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
    let mut queued = false;
    for (path, kind) in stale {
        // Observe once: a still-unreadable file fails without chaining, and
        // the next interval tries again.
        queued |= queue_catalog_validation(
            catalog,
            blind_retry_request(path, kind, MAX_CATALOG_VALIDATION_RETRIES),
        );
    }
    queued
}
