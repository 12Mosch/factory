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

/// Re-observes the path on the worker after classification and reports
/// whether it still resolves to the classified file instance, so the frame
/// can install the outcome without touching the filesystem. Stable identity
/// (device, inode, change time) defeats same-length replacements that
/// preserve mtime; without a stable identity this falls back to
/// length/mtime equality.
fn confirm_path_unchanged(path: &Path, observed: &SaveFileMetadataFingerprint) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let current = save_file_metadata_fingerprint(&file);
    match (&observed.identity, &current.identity) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => current.len == observed.len && current.modified == observed.modified,
    }
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
                fingerprint: None,
                attempt,
                path_confirmed_current: false,
            };
        }
    };
    let metadata = save_file_metadata_fingerprint(&file);
    if metadata.len > SaveLimits::default().max_encoded_bytes {
        let path_confirmed_current = confirm_path_unchanged(&path, &metadata);
        return CatalogValidationOutcome {
            path,
            compatibility: SaveCompatibility::ExceedsCurrentLimits,
            fingerprint: None,
            attempt,
            path_confirmed_current,
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
                let path_confirmed_current = confirm_path_unchanged(&path, &metadata);
                return CatalogValidationOutcome {
                    path,
                    compatibility: SaveCompatibility::ValidationPending,
                    fingerprint: None,
                    attempt,
                    path_confirmed_current,
                };
            }
        },
    };
    if !source_compatibility.can_load() {
        let path_confirmed_current = confirm_path_unchanged(&path, &metadata);
        return CatalogValidationOutcome {
            path,
            compatibility: source_compatibility,
            fingerprint: None,
            attempt,
            path_confirmed_current,
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
            let path_confirmed_current = confirm_path_unchanged(&path, &metadata);
            return CatalogValidationOutcome {
                path,
                compatibility: SaveCompatibility::ExceedsCurrentLimits,
                fingerprint: None,
                attempt,
                path_confirmed_current,
            };
        }
        Err(_) => {
            let path_confirmed_current = confirm_path_unchanged(&path, &metadata);
            return CatalogValidationOutcome {
                path,
                compatibility: SaveCompatibility::ValidationPending,
                fingerprint: None,
                attempt,
                path_confirmed_current,
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
    // Final freshness observation stays on the worker: the frame installs
    // certified outcomes without any filesystem call.
    let path_confirmed_current = confirm_path_unchanged(&path, &after_metadata);
    CatalogValidationOutcome {
        path,
        compatibility,
        fingerprint: stable_fingerprint,
        attempt,
        path_confirmed_current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_confirmation_rejects_same_length_mtime_preserving_replacement() {
        let dir = std::env::temp_dir().join(format!(
            "factory-path-confirm-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        fs::write(&path, vec![0x41u8; 1024]).unwrap();
        let observed = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert!(
            confirm_path_unchanged(&path, &observed),
            "an unmodified path must confirm"
        );
        // Same-length replacement with the mtime restored: length and mtime
        // look identical, but the file instance changed.
        fs::write(&path, vec![0x42u8; 1024]).unwrap();
        if let Some(mtime) = observed.modified {
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(mtime)
                .unwrap();
        }
        let spoofed = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_eq!(
            (spoofed.len, spoofed.modified),
            (observed.len, observed.modified),
            "the test must spoof the signals the old length/mtime check relied on"
        );
        assert!(
            !confirm_path_unchanged(&path, &observed),
            "a same-length mtime-preserving replacement must not inherit the old verdict"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
