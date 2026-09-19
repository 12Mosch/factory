//! Consumption-time path re-confirmation for externally replaced files.
//!
//! Background workers certify the file instance they decoded, but the frame
//! consumes their results later. Our own writer bumps the per-path artifact
//! epoch (see [`super::container`]), which the frame checks without touching
//! the filesystem — but an external actor (cloud sync, another process) can
//! replace the path without bumping it. Consuming a carried certification
//! would then install stale bytes (loads) or publish a stale verdict
//! (catalog validation).
//!
//! This module provides the shared second phase both consumers use: the
//! frame parks a worker-certified result and spawns a lightweight
//! confirmation worker that re-opens the path and compares its current
//! file-instance fingerprint against the certified one. Consumption proceeds
//! only when a confirmation from the same round still matches, chained to
//! the process-local writer epoch so our own commits invalidate it too. The
//! residual window — a replacement landing between the confirmation's final
//! observation and the memory-only install — holds no filesystem I/O or
//! sleeps; see the documented bounds at both install sites.
//!
//! Confirmation workers are fire-and-forget by design: each performs a
//! single open plus metadata fingerprint, sends the verdict through the
//! handle channel, and exits. Dropping a handle (superseded request,
//! cancelled load, catalog mutation, teardown) abandons at most one such
//! bounded worker, which touches no locks and holds no files.

use super::SaveFileMetadataFingerprint;
use super::catalog::inspect::save_file_metadata_fingerprint;
use super::container::save_artifact_epoch;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use std::thread;

/// Whether the path still resolves to the certified file instance. Stable
/// file identity (device, inode, change time) defeats same-length
/// replacements that preserve mtime; without a stable identity this falls
/// back to length/mtime equality. An unopenable path counts as changed so
/// the caller restarts and surfaces the real failure instead of consuming
/// obsolete state.
pub(crate) fn path_resolves_to_instance(
    path: &Path,
    certified: &SaveFileMetadataFingerprint,
) -> bool {
    let current = match std::fs::File::open(path) {
        Ok(file) => save_file_metadata_fingerprint(&file),
        Err(_) => return false,
    };
    match (&certified.identity, &current.identity) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => current.len == certified.len && current.modified == certified.modified,
    }
}

/// Freshness observation from a confirmation worker: whether the path still
/// resolved to the certified instance, and the writer epoch bracketing the
/// observation. Consumers chain this epoch to the worker's certification
/// epoch and the current epoch so an own-writer commit anywhere in between
/// invalidates the confirmation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PathConfirmation {
    pub matched: bool,
    pub epoch: u64,
}

/// Non-blocking poll state of a confirmation worker.
#[derive(Debug)]
pub(crate) enum ConfirmationPoll {
    /// The worker has not reported yet; park and poll again next frame.
    Pending,
    /// The worker confirmed (or rejected) the path in this round.
    Ready(PathConfirmation),
    /// The worker died before reporting; treat as uncertified.
    WorkerGone,
}

/// Channel handle to a confirmation worker. The mutex exists solely for the
/// Bevy resource `Sync` bound; the worker sends exactly once, so polling
/// never contends.
pub(crate) struct PathConfirmationHandle {
    receiver: Mutex<mpsc::Receiver<PathConfirmation>>,
}

impl std::fmt::Debug for PathConfirmationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathConfirmationHandle")
            .finish_non_exhaustive()
    }
}

impl PathConfirmationHandle {
    pub(crate) fn poll(&self) -> ConfirmationPoll {
        match self
            .receiver
            .lock()
            .expect("confirmation channel is never shared")
            .try_recv()
        {
            Ok(confirmation) => ConfirmationPoll::Ready(confirmation),
            Err(mpsc::TryRecvError::Empty) => ConfirmationPoll::Pending,
            Err(mpsc::TryRecvError::Disconnected) => ConfirmationPoll::WorkerGone,
        }
    }
}

/// Spawns a confirmation worker for one certified file instance. The worker
/// brackets its observation with the per-path writer epoch so the consumer
/// can chain certification, confirmation, and installation into one
/// unbroken freshness window.
pub(crate) fn spawn_path_confirmation(
    path: PathBuf,
    certified: SaveFileMetadataFingerprint,
) -> PathConfirmationHandle {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let before = save_artifact_epoch(&path);
        let matched = path_resolves_to_instance(&path, &certified);
        let after = save_artifact_epoch(&path);
        let _ = tx.send(PathConfirmation {
            matched: matched && before == after,
            epoch: after,
        });
    });
    PathConfirmationHandle {
        receiver: Mutex::new(rx),
    }
}
