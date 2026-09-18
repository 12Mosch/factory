//! Background payload validation: bounded worker queue and full loads.
//!
//! Interactive catalog refreshes never decode payloads inline. They queue a
//! [`super::super::CatalogValidationRequest`] per cache miss and run at most
//! one worker; [`super::polling`] collects finished jobs and schedules
//! bounded retries. [`validate_loadable_file`] is the synchronous counterpart
//! for explicit callers such as recovery tests.

use super::super::container::{ContainerError, load_simulation_from_reader};
use super::super::{
    CachedSaveValidation, CatalogValidationJob, CatalogValidationOutcome, CatalogValidationRequest,
    SaveCatalog, SaveCompatibility, SaveFileMetadataFingerprint, SaveKind,
};
use super::inspect::{classify_open_save, save_file_fingerprint, save_file_metadata_fingerprint};
use factory_sim::SaveLimits;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

pub(crate) const MAX_CATALOG_VALIDATION_JOBS: usize = 1;
/// Number of retries after the initial validation attempt. Attempts are
/// numbered from zero and only attempts below this bound schedule a retry,
/// so at most three validations (attempts 0, 1, 2) run per refresh cycle.
/// Transient failures (brief locks, mid-sync replacement) are retried;
/// anything still unstable afterwards waits for the next refresh or pending
/// rescan instead of spinning the worker.
pub(crate) const MAX_CATALOG_VALIDATION_RETRIES: u8 = 2;

/// Queues a validation request unless the same file version is already
/// queued or running. Returns whether the queue changed.
pub(crate) fn queue_catalog_validation(
    catalog: &mut SaveCatalog,
    request: CatalogValidationRequest,
) -> bool {
    if catalog
        .validation_jobs
        .iter()
        .any(|job| job.path == request.path && job.metadata == request.metadata)
    {
        return false;
    }
    if catalog
        .validation_queue
        .iter()
        .any(|queued| queued.path == request.path && queued.metadata == request.metadata)
    {
        return false;
    }
    catalog
        .validation_queue
        .retain(|queued| queued.path != request.path);
    catalog.validation_queue.push_back(request);
    true
}

/// Builds a validation request for a file that could not be opened for
/// fingerprinting. The worker classifies from its own handle on arrival, so
/// the placeholder compatibility and identity below are never published.
pub(crate) fn blind_retry_request(
    path: PathBuf,
    kind: SaveKind,
    attempt: u8,
) -> CatalogValidationRequest {
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

/// Starts queued validations up to the job bound. Returns whether any job
/// started.
pub(crate) fn start_catalog_validation_jobs(catalog: &mut SaveCatalog) -> bool {
    let mut started = false;
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
        started = true;
    }
    started
}

/// Wraps a save handle so catalog shutdown can abort hashing and decoding
/// at the next read chunk instead of waiting for the remainder.
pub(crate) struct CancelReader<R> {
    pub(crate) reader: R,
    pub(crate) cancel: Arc<AtomicBool>,
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

/// Fully validates a loadable payload synchronously for explicit callers such
/// as recovery tests. Interactive catalog refreshes use the background path.
/// The header is always classified from the same open handle that is
/// fingerprinted below, so a replacement between catalog inspection and this
/// call cannot pair a stale classification with the current bytes.
pub(crate) fn validate_loadable_file(
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

pub(crate) fn validate_loadable_path(
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
        _ => match factory_data::PrototypeCatalog::load_base()
            .ok()
            .map(|catalog| factory_sim::prototype_hash(&catalog))
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
