//! Bevy polling: collect finished validation workers, confirm parked
//! verdicts against same-round path observations, schedule retries, and
//! re-observe stale pending entries.
//!
//! The inner poll returns whether externally relevant catalog state
//! changed; pure timer bookkeeping does not count, so an idle catalog never
//! trips Bevy change detection.

use super::super::container::save_artifact_epoch;
use super::super::freshness::{ConfirmationPoll, spawn_path_confirmation};
use super::super::{
    CachedSaveValidation, ConfirmingValidation, SaveCatalog, SaveCompatibility, SaveKind,
};
use super::validation::{
    MAX_CATALOG_VALIDATION_RETRIES, blind_retry_request, queue_catalog_validation,
    start_catalog_validation_jobs,
};
use bevy::log::warn;
use bevy::prelude::{DetectChangesMut, ResMut};
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
        // No filesystem work on the frame schedule: the worker re-observed
        // the path after classifying and certified the outcome only when it
        // still resolved to the same file instance. Stable identity defeats
        // same-length replacements that preserve mtime; uncertified
        // outcomes always take the retry path below. Payload-derived
        // verdicts (compatible, migratable, and corruption — all produced
        // by decoding bytes) additionally require a stable fingerprint: a
        // missing one means the pre/post decode observations disagreed
        // (e.g. an in-place rewrite mid-validation), so the verdict is not
        // tied to one stable byte sequence. Header-only verdicts (oversized
        // or unloadable headers, which never decode) install certified. The
        // writer epoch rejects verdicts overtaken by our own save commits
        // between certification and installation.
        let payload_stable = outcome.fingerprint.is_some()
            || !matches!(
                outcome.compatibility,
                SaveCompatibility::Compatible
                    | SaveCompatibility::MigratableSaveFormat { .. }
                    | SaveCompatibility::CorruptOrTruncated
            );
        if outcome.path_confirmed_current
            && outcome.commit_epoch == save_artifact_epoch(&outcome.path)
            && outcome.compatibility != SaveCompatibility::ValidationPending
            && payload_stable
        {
            // Park for same-round confirmation instead of installing: an
            // external replacement after the worker's certification must not
            // inherit this verdict (see `super::super::freshness`). The
            // confirmation worker re-observes the path against the bound
            // instance below; installation happens in `consume_confirmations`
            // only on its same-round verdict. Starting confirmation work
            // counts as changed, like starting a validation job.
            if let Some(certified) = outcome.observed_metadata.clone() {
                let path = outcome.path.clone();
                catalog.confirming.push(ConfirmingValidation {
                    outcome,
                    confirm: spawn_path_confirmation(path, certified),
                });
                changed = true;
            } else {
                // No bound observation (the worker never opened the file):
                // without an instance to confirm against, the verdict cannot
                // be installed. The entry stays pending and the rescan
                // machinery re-observes it.
                warn!(
                    "catalog validation for {} has no bound observation; retrying",
                    outcome.path.display()
                );
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
            // Retry on the worker, which classifies from its own handle.
            // No file opens on the frame schedule.
            let kind = catalog.entries[entry_index].metadata.kind.clone();
            let next_attempt = outcome.attempt + 1;
            changed |= queue_catalog_validation(
                catalog,
                blind_retry_request(outcome.path, kind, next_attempt),
            );
        }
    }
    changed |= consume_confirmations(catalog);
    changed |= rescan_stale_pending(catalog);
    changed |= start_catalog_validation_jobs(catalog);
    changed
}

/// Installs parked verdicts whose same-round path confirmation still
/// resolves to the certified instance with an unbroken writer-epoch chain.
/// A mismatch — external replacement or deletion after the validation
/// worker's certification, or an own-writer commit in between — drops the
/// verdict while the entry stays pending, so the file converges on a fresh
/// verdict through the pending rescan instead of inheriting this one.
/// Returns whether any verdict installed.
///
/// Residual bound: a replacement landing between the confirmation worker's
/// final observation and the memory-only installation below still publishes
/// the old verdict. That window holds no filesystem I/O or sleeps — the
/// frame consumes the finished confirmation and installs in the same poll —
/// and any verdict that survives it is re-checked against a fresh
/// observation on the next refresh or rescan cycle.
fn consume_confirmations(catalog: &mut SaveCatalog) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index < catalog.confirming.len() {
        let confirmation = match catalog.confirming[index].confirm.poll() {
            ConfirmationPoll::Pending => {
                index += 1;
                continue;
            }
            ConfirmationPoll::Ready(confirmation) => confirmation,
            ConfirmationPoll::WorkerGone => {
                // The confirmation worker died: without a same-round
                // verdict the outcome cannot be installed. The entry stays
                // pending and the rescan machinery re-observes it.
                catalog.confirming.swap_remove(index);
                continue;
            }
        };
        let outcome = catalog.confirming.swap_remove(index).outcome;
        // Unbroken freshness chain: no own-writer commit between the
        // validation worker's final check, the confirmation's observation,
        // and this installation, and the path still resolves to the
        // certified instance.
        if !confirmation.matched
            || outcome.commit_epoch != confirmation.epoch
            || confirmation.epoch != save_artifact_epoch(&outcome.path)
        {
            continue;
        }
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
    }
    changed
}

/// Re-observes entries stuck at `ValidationPending` with no queued or
/// running validation, at most every `PENDING_RESCAN_INTERVAL`. This
/// covers attempt exhaustion and refresh-time open failures, so recovered
/// access is picked up without waiting for an unrelated catalog refresh.
/// Each rescan performs a single observation per entry; a still-failing
/// entry simply waits for the next interval. Returns whether any request
/// was queued; advancing the throttle alone does not count.
pub(crate) fn rescan_stale_pending(catalog: &mut SaveCatalog) -> bool {
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
                // A parked confirmation is active work: its verdict installs
                // (or drops) within a round, so a second full payload
                // validation here would duplicate decoding for large saves.
                && !catalog
                    .confirming
                    .iter()
                    .any(|confirming| confirming.outcome.path == entry.path)
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
