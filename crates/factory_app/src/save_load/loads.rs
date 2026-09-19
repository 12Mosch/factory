//! Bounded asynchronous load jobs.
//!
//! File reading, decoding, and validation run on at most one background
//! worker. The frame schedule only enqueues requests and installs a validated
//! candidate at a controlled boundary through the shared
//! [`crate::save_load::enter_swapped_world`] lifecycle. Results carry request
//! and world-generation ids so stale or out-of-order completions cannot
//! install over a newer world.

use super::SaveFileMetadataFingerprint;
use super::SaveId;
use super::catalog::inspect::save_file_metadata_fingerprint;
use super::catalog::validation::CancelReader;
use super::container::{ContainerError, load_simulation_from_reader, save_artifact_epoch};
use super::lifecycle::{
    LoadJobError, LoadJobPhase, MAX_LOAD_WORKERS, MAX_QUEUED_LOADS, PersistenceRequestId,
};
use factory_sim::{SaveLimits, Simulation};
use std::collections::VecDeque;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::thread::{self, JoinHandle};

/// Bounded asynchronous load queue. Newer requests supersede older queued
/// (not yet started) loads; a running load installs only when still current.
#[derive(bevy::prelude::Resource, Default)]
pub struct PendingLoadJobs {
    running: Option<RunningLoad>,
    queue: VecDeque<QueuedLoad>,
    ready: Option<ReadyLoad>,
    latest_request: Option<PersistenceRequestId>,
    last_installed: Option<PersistenceRequestId>,
}

pub(crate) struct QueuedLoad {
    pub request_id: PersistenceRequestId,
    pub id: SaveId,
    pub display_name: String,
    pub path: PathBuf,
    pub observed_generation: u64,
}

struct RunningLoad {
    request_id: PersistenceRequestId,
    id: SaveId,
    display_name: String,
    observed_generation: u64,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicBool>,
    handle: JoinHandle<Result<LoadCandidate, LoadJobError>>,
}

/// Validated off-thread candidate waiting for boundary installation.
pub(crate) struct LoadCandidate {
    pub request_id: PersistenceRequestId,
    pub observed_generation: u64,
    pub simulation: Simulation,
    pub tick: u64,
    pub player_tile: (f32, f32),
    pub path: PathBuf,
    /// Writer epoch for the target observed before the worker opened it. A
    /// save committed afterwards replaces the decoded bytes, so the frame
    /// restarts the request instead of installing a rollback.
    pub artifact_epoch: u64,
    /// File identity of the open handle the worker decoded. An external
    /// actor (cloud sync, another process) can replace the path without
    /// bumping the process-local epoch; the worker re-observes the path
    /// after decoding and records the verdict below, since the frame-side
    /// boundary must stay memory-only.
    pub observed_identity: SaveFileMetadataFingerprint,
    /// Off-thread replacement certification: the path still resolved to
    /// [`Self::observed_identity`] when decoding finished. The install
    /// boundary restarts uncertified candidates instead of installing
    /// obsolete bytes.
    pub end_certified: bool,
}

/// A validated candidate retained with its catalog identity for boundary
/// installation. Retained across frames when the simulation is busy. The
/// generation lives on the candidate; no duplicate is stored here.
pub(crate) struct ReadyLoad {
    pub id: SaveId,
    pub display_name: String,
    pub request_id: PersistenceRequestId,
    pub path: PathBuf,
    pub candidate: LoadCandidate,
}

impl ReadyLoad {
    /// Whether a save committed after the worker opened its handle,
    /// replacing the decoded bytes. Overtaken candidates must restart
    /// instead of installing a rollback.
    pub fn is_overtaken(&self) -> bool {
        super::container::save_artifact_epoch(&self.path) != self.candidate.artifact_epoch
    }
}

pub(crate) struct CompletedLoad {
    pub id: SaveId,
    pub display_name: String,
    pub request_id: PersistenceRequestId,
    pub observed_generation: u64,
    pub result: Result<LoadCandidate, LoadJobError>,
}

impl PendingLoadJobs {
    pub fn is_empty(&self) -> bool {
        self.running.is_none() && self.queue.is_empty() && self.ready.is_none()
    }

    pub fn any_running(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|job| !job.handle.is_finished())
    }

    pub fn has_ready_candidate(&self) -> bool {
        self.ready.is_some()
    }

    pub fn is_id_pending(&self, id: &SaveId) -> bool {
        if self.ready.as_ref().is_some_and(|ready| &ready.id == id) {
            return true;
        }
        if self
            .running
            .as_ref()
            .is_some_and(|job| &job.id == id && !job.handle.is_finished())
        {
            return true;
        }
        self.queue.iter().any(|queued| &queued.id == id)
    }

    pub fn pending_ids(&self) -> Vec<SaveId> {
        let mut ids = Vec::new();
        if let Some(running) = &self.running {
            ids.push(running.id.clone());
        }
        ids.extend(self.queue.iter().map(|queued| queued.id.clone()));
        if let Some(ready) = &self.ready {
            ids.push(ready.id.clone());
        }
        ids
    }

    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    /// Current load phases in execution order, polled without blocking.
    pub fn progress(&self) -> Vec<(SaveId, LoadJobPhase)> {
        let mut phases = Vec::new();
        if let Some(running) = &self.running {
            phases.push((
                running.id.clone(),
                LoadJobPhase::decode(running.phase.load(Ordering::Relaxed)),
            ));
        }
        phases.extend(
            self.queue
                .iter()
                .map(|queued| (queued.id.clone(), LoadJobPhase::Queued)),
        );
        if let Some(ready) = &self.ready {
            phases.push((ready.id.clone(), LoadJobPhase::ReadyToInstall));
        }
        phases
    }

    /// Latest accepted request id, if any.
    pub fn latest_request(&self) -> Option<PersistenceRequestId> {
        self.latest_request
    }

    /// Whether a completed load is obsolete. Any completion that is not the
    /// latest accepted request is superseded — even if the newer worker
    /// already finished. The latest request stays authoritative until an
    /// even newer request arrives, so an older completion must never
    /// install first and poison the newer result's generation check.
    pub fn completion_superseded(&self, request_id: PersistenceRequestId) -> bool {
        self.latest_request
            .is_some_and(|latest| request_id != latest)
    }

    /// Cancels a queued or ready load and signals a running load when it
    /// matches. Cancellation never mutates the active world.
    pub fn cancel(&mut self, id: &SaveId) -> bool {
        let mut cancelled = false;
        let before = self.queue.len();
        self.queue.retain(|queued| &queued.id != id);
        cancelled |= self.queue.len() != before;
        if self.ready.as_ref().is_some_and(|ready| &ready.id == id) {
            self.ready = None;
            cancelled = true;
        }
        if let Some(running) = &self.running
            && &running.id == id
            && !running.handle.is_finished()
        {
            running.cancel.store(true, Ordering::Relaxed);
            cancelled = true;
        }
        cancelled
    }

    /// Cancels everything pending. New-world installation calls this so a
    /// previously requested load cannot install over the newer world.
    /// Retires the authoritative latest request as well: otherwise a queued
    /// newer load removed here keeps naming `latest_request`, the running
    /// older load's cancellation is filtered as superseded, and its
    /// `Loading...` status is orphaned forever with nothing left to clear
    /// it.
    pub fn cancel_all(&mut self) {
        self.queue.clear();
        self.ready = None;
        self.latest_request = None;
        if let Some(running) = &self.running
            && !running.handle.is_finished()
        {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn take_ready_for_install(&mut self) -> Option<ReadyLoad> {
        self.ready.take()
    }

    pub(crate) fn retain_ready(&mut self, ready: ReadyLoad) {
        self.ready = Some(ready);
    }

    pub(crate) fn note_installed(&mut self, request_id: PersistenceRequestId) {
        self.last_installed = Some(request_id);
    }

    /// After an older load installs, a queued newer load must not look stale:
    /// rebase its observation to the new current generation.
    pub(crate) fn rebase_queued_observations(&mut self, generation: u64) {
        for queued in &mut self.queue {
            queued.observed_generation = generation;
        }
    }

    fn start_next(&mut self) {
        const {
            assert!(MAX_LOAD_WORKERS >= 1);
        }
        if self.running.is_some() {
            return;
        }
        let Some(queued) = self.queue.pop_front() else {
            return;
        };
        let phase = Arc::new(AtomicU8::new(LoadJobPhase::Queued.encode()));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_phase = Arc::clone(&phase);
        let worker_cancel = Arc::clone(&cancel);
        let request_id = queued.request_id;
        let path = queued.path.clone();
        let handle =
            thread::spawn(move || run_load_worker(request_id, path, worker_phase, worker_cancel));
        self.running = Some(RunningLoad {
            request_id,
            id: queued.id,
            display_name: queued.display_name,
            observed_generation: queued.observed_generation,
            phase,
            cancel,
            handle,
        });
    }

    fn join_running(&mut self) {
        if let Some(job) = self.running.take() {
            let _ = join_load(job.handle);
        }
    }
}

impl Drop for PendingLoadJobs {
    fn drop(&mut self) {
        // Queued and ready loads never installed, so dropping them cancels
        // without touching the world. Signal the running decoder first: it
        // is cancellation-aware (aborts at the next read chunk), so teardown
        // latency does not scale with the remaining decode work. Joining
        // afterwards stays deterministic: no detached worker can outlive the
        // queue and hold save files.
        self.queue.clear();
        self.ready = None;
        if let Some(running) = &self.running
            && !running.handle.is_finished()
        {
            running.cancel.store(true, Ordering::Relaxed);
        }
        self.join_running();
    }
}

/// Enqueues an asynchronous load. Returns the request id when accepted.
/// Newest wins: an accepted request supersedes older queued loads and any
/// retained older candidate, so obsolete loads never consume decode work or
/// install over the requested world.
pub(crate) fn queue_load(
    id: SaveId,
    display_name: String,
    path: PathBuf,
    observed_generation: u64,
    pending: &mut PendingLoadJobs,
) -> PersistenceRequestId {
    let request_id = PersistenceRequestId::next();
    // Request ids are monotonic, so every queued entry is older than the new
    // request and is superseded immediately.
    pending.queue.clear();
    if pending
        .ready
        .as_ref()
        .is_some_and(|ready| ready.request_id < request_id)
    {
        pending.ready = None;
    }
    pending.queue.push_back(QueuedLoad {
        request_id,
        id,
        display_name,
        path,
        observed_generation,
    });
    debug_assert!(
        pending.queue.len() <= MAX_QUEUED_LOADS,
        "queued loads must stay within the documented bound"
    );
    pending.latest_request = Some(request_id);
    pending.start_next();
    request_id
}

/// Decodes a save through a cancellation-aware reader so `cancel_all`
/// (e.g. new-world installation) aborts a stale decode at the next read
/// chunk instead of running it to completion. Reads fail with
/// `ConnectionAborted` once the signal is set; see [`map_load_io_error`].
/// Takes the already-open handle so the caller can bind the decoded bytes
/// to that file instance (see `run_load_worker`).
fn load_simulation_cancellable(
    file: fs::File,
    cancel: &Arc<AtomicBool>,
) -> Result<Simulation, ContainerError> {
    let mut reader = BufReader::new(CancelReader {
        reader: file,
        cancel: Arc::clone(cancel),
    });
    load_simulation_from_reader(&mut reader, SaveLimits::default())
}

/// Maps decode I/O failures. A failure observed while the cancellation
/// signal is set reports `Cancelled` — the result is unwanted either way,
/// and silent cancellation must win over a transient-error status.
/// Otherwise transient denials stay retryable and are never corruption.
fn map_load_io_error(io_error: std::io::Error, cancel: &AtomicBool) -> LoadJobError {
    if cancel.load(Ordering::Relaxed) {
        return LoadJobError::Cancelled;
    }
    match io_error.kind() {
        std::io::ErrorKind::NotFound
        | std::io::ErrorKind::PermissionDenied
        | std::io::ErrorKind::WouldBlock
        | std::io::ErrorKind::TimedOut
        | std::io::ErrorKind::Interrupted
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::ConnectionReset => LoadJobError::TransientIo(io_error.to_string()),
        _ => LoadJobError::Io(io_error.to_string()),
    }
}

fn run_load_worker(
    request_id: PersistenceRequestId,
    path: PathBuf,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicBool>,
) -> Result<LoadCandidate, LoadJobError> {
    phase.store(LoadJobPhase::Reading.encode(), Ordering::Relaxed);
    if cancel.load(Ordering::Relaxed) {
        return Err(LoadJobError::Cancelled);
    }
    phase.store(LoadJobPhase::Decoding.encode(), Ordering::Relaxed);
    // Record the writer epoch before opening: any commit afterwards (even
    // between this read and the open) only causes a harmless restart that
    // re-decodes the committed bytes, never a rollback install.
    let artifact_epoch = save_artifact_epoch(&path);
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(io_error) => return Err(map_load_io_error(io_error, &cancel)),
    };
    // Bind the decoded bytes to this open handle: an external replacement
    // afterwards changes what the path resolves to, and the install
    // boundary restarts instead of installing the old instance's bytes.
    let observed_identity = save_file_metadata_fingerprint(&file);
    let simulation = load_simulation_cancellable(file, &cancel).map_err(|error| match error {
        ContainerError::Io(io_error) => map_load_io_error(io_error, &cancel),
        ContainerError::TooLarge => LoadJobError::TooLarge,
        ContainerError::Simulation(error) => super::map_load_error(error),
        ContainerError::UnsupportedVersion(found) => {
            LoadJobError::Corrupt(format!("unsupported container version {found}"))
        }
        ContainerError::MetadataTooLarge(size) => {
            LoadJobError::Corrupt(format!("metadata is {size} bytes"))
        }
        ContainerError::MetadataEncoding(detail) => LoadJobError::Corrupt(detail),
        ContainerError::Truncated => LoadJobError::Corrupt("container is truncated".into()),
        ContainerError::InvalidContainerMagic => {
            LoadJobError::Corrupt("file is not a Factory save".into())
        }
        // The load path never threads a cancellation flag, so this is
        // unreachable; map it to silent cancellation rather than
        // corruption if that ever changes.
        ContainerError::Cancelled => LoadJobError::Cancelled,
    })?;
    if cancel.load(Ordering::Relaxed) {
        return Err(LoadJobError::Cancelled);
    }
    // No second validation here: both decode paths
    // (`load_from_reader_with_limits` and the record loader) already run
    // `validate_state` before returning, with identical corruption mapping.
    if cancel.load(Ordering::Relaxed) {
        return Err(LoadJobError::Cancelled);
    }
    // Off-thread replacement certification, carried with the candidate: an
    // external actor can replace the path without bumping the process-local
    // writer epoch, so re-observe it now (still on the worker) and record
    // the verdict. The frame-side install boundary stays memory-only and
    // restarts on an uncertified candidate instead of installing obsolete
    // bytes.
    let end_certified = certify_path_unchanged(&path, &observed_identity);
    let tick = simulation.tick_count();
    let player_tile = simulation.player().position_tiles();
    phase.store(LoadJobPhase::ReadyToInstall.encode(), Ordering::Relaxed);
    Ok(LoadCandidate {
        request_id,
        // Filled by the collector from the running job record; the worker
        // never observes the live world generation.
        observed_generation: 0,
        simulation,
        tick,
        player_tile,
        path,
        artifact_epoch,
        observed_identity,
        end_certified,
    })
}

/// Whether the path still resolves to the decoded file instance: re-opens
/// the path and compares against the open handle's fingerprint (stable file
/// identity when available, length plus mtime otherwise). An unopenable
/// path counts as changed so the caller restarts and surfaces the real
/// failure instead of installing obsolete bytes.
pub(crate) fn certify_path_unchanged(
    path: &Path,
    open_identity: &SaveFileMetadataFingerprint,
) -> bool {
    let current = match fs::File::open(path) {
        Ok(file) => save_file_metadata_fingerprint(&file),
        Err(_) => return false,
    };
    match (&open_identity.identity, &current.identity) {
        (Some(expected), Some(actual)) => expected == actual,
        _ => current.len == open_identity.len && current.modified == open_identity.modified,
    }
}

pub(crate) fn take_completed_loads(pending: &mut PendingLoadJobs) -> Vec<CompletedLoad> {
    let mut completed = Vec::new();
    if let Some(job) = pending.running.take() {
        if job.handle.is_finished() {
            let result = join_load(job.handle).map(|mut candidate| {
                candidate.observed_generation = job.observed_generation;
                candidate.request_id = job.request_id;
                candidate
            });
            completed.push(CompletedLoad {
                id: job.id,
                display_name: job.display_name,
                request_id: job.request_id,
                observed_generation: job.observed_generation,
                result,
            });
        } else {
            pending.running = Some(job);
        }
    }
    pending.start_next();
    completed
}

fn join_load(
    handle: JoinHandle<Result<LoadCandidate, LoadJobError>>,
) -> Result<LoadCandidate, LoadJobError> {
    handle
        .join()
        .unwrap_or_else(|_| Err(LoadJobError::Corrupt("load worker panicked".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_queued_load_supersedes_older_ones() {
        let mut pending = PendingLoadJobs::default();
        // Occupy the worker with a running job so enqueues stay queued.
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let handle = thread::spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Err(LoadJobError::Cancelled)
        });
        started_rx.recv().unwrap();
        pending.running = Some(RunningLoad {
            request_id: PersistenceRequestId::next(),
            id: SaveId::new("running"),
            display_name: "Running".into(),
            observed_generation: 0,
            phase: Arc::new(AtomicU8::new(LoadJobPhase::Reading.encode())),
            cancel: Arc::new(AtomicBool::new(false)),
            handle,
        });
        for index in 0..(MAX_QUEUED_LOADS + 2) {
            queue_load(
                SaveId::new(format!("queued-{index}")),
                format!("Queued {index}"),
                PathBuf::from(format!("queued-{index}.factsim")),
                0,
                &mut pending,
            );
        }
        // Newest wins: at most one queued load survives, and it is the
        // newest request, so obsolete loads never consume decode work.
        assert_eq!(
            pending.queued_len(),
            MAX_QUEUED_LOADS,
            "only the newest queued load is retained"
        );
        assert_eq!(
            pending
                .queue
                .back()
                .map(|queued| queued.id.as_str().to_owned()),
            Some(format!("queued-{}", MAX_QUEUED_LOADS + 1)),
            "the newest queued load must survive superseding"
        );
        release_tx.send(()).unwrap();
        pending.join_running();
    }

    #[test]
    fn load_progress_reports_phases_without_blocking() {
        let mut pending = PendingLoadJobs::default();
        queue_load(
            SaveId::new("first"),
            "First".into(),
            PathBuf::from("first.factsim"),
            3,
            &mut pending,
        );
        // With no contention the first request starts immediately; progress
        // reports the running job by id, reaching a non-queued phase once
        // the worker stores it.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let phases = loop {
            let phases = pending.progress();
            if phases.len() == 1
                && phases[0].0 == SaveId::new("first")
                && phases[0].1 != LoadJobPhase::Queued
            {
                break phases;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "load worker did not report progress"
            );
            thread::yield_now();
        };
        assert_eq!(phases.len(), 1);
        pending.cancel_all();
        pending.join_running();
    }

    #[test]
    fn finished_newer_worker_still_supersedes_older_completion() {
        // An older completion must not install once a newer request was
        // accepted, even if the newer worker already finished before the
        // older completion is collected: installing the older world would
        // bump the generation and discard the newer result as stale.
        let mut pending = PendingLoadJobs::default();
        let older = PersistenceRequestId::next();
        let newer = PersistenceRequestId::next();
        pending.latest_request = Some(newer);
        let handle = thread::spawn(|| Err(LoadJobError::Cancelled));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !handle.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "newer load worker did not finish"
            );
            thread::yield_now();
        }
        pending.running = Some(RunningLoad {
            request_id: newer,
            id: SaveId::new("newer"),
            display_name: "Newer".into(),
            observed_generation: 0,
            phase: Arc::new(AtomicU8::new(LoadJobPhase::ReadyToInstall.encode())),
            cancel: Arc::new(AtomicBool::new(false)),
            handle,
        });
        assert!(
            pending.completion_superseded(older),
            "an older completion must stay superseded after its newer worker finished"
        );
        assert!(
            !pending.completion_superseded(newer),
            "the latest request stays authoritative once finished"
        );
        pending.join_running();
    }

    #[test]
    fn overtaken_candidate_is_detected_by_writer_epoch() {
        use super::super::container::{bump_save_artifact_epoch, save_artifact_epoch};
        use super::SaveFileMetadataFingerprint;
        let path = PathBuf::from("overtaken.factsim");
        let candidate = |epoch| LoadCandidate {
            request_id: PersistenceRequestId::next(),
            observed_generation: 0,
            simulation: Simulation::new_test_world(11),
            tick: 0,
            player_tile: (0.0, 0.0),
            path: path.clone(),
            artifact_epoch: epoch,
            observed_identity: SaveFileMetadataFingerprint {
                len: 0,
                modified: None,
                identity: None,
            },
            end_certified: true,
        };
        let ready = |epoch| ReadyLoad {
            id: SaveId::new("quicksave"),
            display_name: "Quicksave".into(),
            request_id: PersistenceRequestId::next(),
            path: path.clone(),
            candidate: candidate(epoch),
        };
        // No commit since the worker opened: current, installable.
        assert!(!ready(save_artifact_epoch(&path)).is_overtaken());
        // A commit afterwards replaces the decoded bytes: restart. The
        // commit sequence starts at 1, so epoch 0 is always stale here.
        bump_save_artifact_epoch(&path);
        assert!(ready(0).is_overtaken());
        let stale_epoch = save_artifact_epoch(&path);
        bump_save_artifact_epoch(&path);
        assert!(
            ready(stale_epoch).is_overtaken(),
            "a candidate overtaken by a writer commit must restart"
        );
    }

    #[test]
    fn replacement_certification_detects_external_replacement() {
        // The worker-side certification behind `end_certified`: re-observing
        // the path off-thread. Unmodified, replaced, and removed paths are
        // observed synchronously here, so no decode race is involved.
        let dir = std::env::temp_dir().join(format!(
            "factory-load-identity-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        std::fs::write(&path, vec![0x41u8; 1024]).unwrap();
        let observed = save_file_metadata_fingerprint(&std::fs::File::open(&path).unwrap());
        // Unmodified path still resolves to the decoded instance.
        assert!(
            certify_path_unchanged(&path, &observed),
            "an unmodified path must still resolve to the decoded instance"
        );
        // External atomic replacement (new inode): the old bytes must not
        // install even though the process-local epoch never moved.
        let replacement = dir.join("replacement.factsim");
        std::fs::write(&replacement, vec![0x42u8; 1024]).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert!(
            !certify_path_unchanged(&path, &observed),
            "an externally replaced path must fail certification"
        );
        // A removed path fails closed: the restart surfaces the real
        // failure instead of installing obsolete bytes.
        std::fs::remove_file(&path).unwrap();
        assert!(
            !certify_path_unchanged(&path, &observed),
            "a removed path must fail certification"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn drop_signals_running_decoder_before_joining() {
        // Teardown must signal the decoder's cancel token before joining:
        // decoding aborts at the next read chunk instead of running to
        // completion, so shutdown latency does not scale with the remaining
        // work. The worker self-bounds its wait so a regression fails the
        // test instead of hanging the suite.
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let observed = Arc::new(AtomicBool::new(false));
        let worker_observed = Arc::clone(&observed);
        let handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !worker_cancel.load(Ordering::Relaxed) {
                if std::time::Instant::now() >= deadline {
                    return Err(LoadJobError::Corrupt("test decoder timeout".into()));
                }
                thread::yield_now();
            }
            worker_observed.store(true, Ordering::Relaxed);
            Err(LoadJobError::Cancelled)
        });
        let mut pending = PendingLoadJobs::default();
        pending.running = Some(RunningLoad {
            request_id: PersistenceRequestId::next(),
            id: SaveId::new("teardown"),
            display_name: "Teardown".into(),
            observed_generation: 0,
            phase: Arc::new(AtomicU8::new(LoadJobPhase::Reading.encode())),
            cancel,
            handle,
        });
        let start = std::time::Instant::now();
        drop(pending);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "drop joined the decoder without signalling cancellation first"
        );
        assert!(
            observed.load(Ordering::Relaxed),
            "the decoder never observed the teardown signal"
        );
    }

    #[test]
    fn cancel_all_retires_latest_request() {
        // A queued newer load removed by `cancel_all` must not keep naming
        // `latest_request`: otherwise the running older load's cancellation
        // is filtered as superseded and its `Loading...` status is orphaned
        // with nothing left to clear it.
        let mut pending = PendingLoadJobs::default();
        pending.latest_request = Some(PersistenceRequestId::next());
        pending.cancel_all();
        assert_eq!(
            pending.latest_request(),
            None,
            "cancel_all must retire the authoritative latest request"
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn cancelled_decode_reports_cancelled_not_transient() {
        // A decode failure observed while the cancellation signal is set
        // must report Cancelled (silently dropped) rather than a
        // transient-error status nobody will retry.
        let cancelled = AtomicBool::new(true);
        let aborted = std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "catalog validation cancelled",
        );
        assert!(matches!(
            map_load_io_error(aborted, &cancelled),
            LoadJobError::Cancelled
        ));
        let missing = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "save is no longer in the catalog",
        );
        assert!(matches!(
            map_load_io_error(missing, &cancelled),
            LoadJobError::Cancelled
        ));
        // Without cancellation the mapping is unchanged: transient
        // denials stay retryable, other I/O stays a hard error.
        let live = AtomicBool::new(false);
        let aborted = std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "reset");
        assert!(matches!(
            map_load_io_error(aborted, &live),
            LoadJobError::TransientIo(_)
        ));
        let eof = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "cut");
        assert!(matches!(map_load_io_error(eof, &live), LoadJobError::Io(_)));
    }
}
