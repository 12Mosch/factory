//! Bounded asynchronous load jobs.
//!
//! File reading, decoding, and validation run on at most one background
//! worker. The frame schedule only enqueues requests and installs a validated
//! candidate at a controlled boundary through the shared
//! [`crate::save_load::enter_swapped_world`] lifecycle. Results carry request
//! and world-generation ids so stale or out-of-order completions cannot
//! install over a newer world.

use super::SaveId;
use super::container::{ContainerError, load_simulation};
use super::lifecycle::{
    LoadJobError, LoadJobPhase, MAX_LOAD_WORKERS, MAX_QUEUED_LOADS, PersistenceRequestId,
};
use factory_sim::Simulation;
use std::collections::VecDeque;
use std::path::PathBuf;
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
}

/// A validated candidate retained with its catalog identity for boundary
/// installation. Retained across frames when the simulation is busy. The
/// generation lives on the candidate; no duplicate is stored here.
pub(crate) struct ReadyLoad {
    pub id: SaveId,
    pub display_name: String,
    pub request_id: PersistenceRequestId,
    pub candidate: LoadCandidate,
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

    /// Whether a newer request is still queued or running after `request_id`.
    pub fn has_newer_queued_or_running(&self, request_id: PersistenceRequestId) -> bool {
        if self
            .running
            .as_ref()
            .is_some_and(|job| job.request_id > request_id && !job.handle.is_finished())
        {
            return true;
        }
        self.queue
            .iter()
            .any(|queued| queued.request_id > request_id)
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
    pub fn cancel_all(&mut self) {
        self.queue.clear();
        self.ready = None;
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
        // without touching the world. Join the running decoder so shutdown
        // does not detach file I/O.
        self.queue.clear();
        self.ready = None;
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
    let simulation = load_simulation(&path).map_err(|error| match error {
        ContainerError::Io(io_error) => match io_error.kind() {
            // Transient denials stay retryable and are never corruption.
            std::io::ErrorKind::NotFound
            | std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset => {
                LoadJobError::TransientIo(io_error.to_string())
            }
            _ => LoadJobError::Io(io_error.to_string()),
        },
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
    })
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
}
