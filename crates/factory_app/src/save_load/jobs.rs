use super::catalog::now_unix_ms;
use super::container::{METADATA_SCHEMA_VERSION, write_save_snapshot};
use super::lifecycle::{
    MAX_QUEUED_SAVES, MAX_RETAINED_SAVE_GENERATIONS, MAX_SAVE_WORKERS, PersistenceRequestId,
    SaveJobError, SaveJobPhase,
};
use super::{
    SaveId, SaveKind, SaveLoadConfig, SaveLoadMetrics, SaveLoadStatus, SaveLoadStatusKind,
    SaveMetadata,
};
use crate::resources::{SimResource, SnapshotSource};
use factory_sim::try_capture_record_snapshot;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Instant;

struct SnapshotCaptureActivity<'a>(&'a AtomicU64);

impl<'a> SnapshotCaptureActivity<'a> {
    fn begin(active_captures: &'a AtomicU64) -> Self {
        active_captures.fetch_add(1, Ordering::AcqRel);
        Self(active_captures)
    }
}

impl Drop for SnapshotCaptureActivity<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Bounded asynchronous save queue.
///
/// At most [`MAX_SAVE_WORKERS`] worker runs at a time and retains one
/// immutable snapshot generation. Accepted manual saves wait in a bounded
/// FIFO ([`MAX_QUEUED_SAVES`]); autosaves coalesce by target and apply
/// backpressure by dropping instead of queueing. Submission never blocks on
/// the simulation lock: the worker acquires it in the background, so frame
/// schedules stay responsive while capture, encoding, and disk I/O proceed.
///
/// Queued entries hold only request parameters plus cheap simulation handles;
/// snapshot memory is retained solely by the running worker after capture.
#[derive(bevy::prelude::Resource, Default)]
pub struct PendingSaveJobs {
    running: Option<RunningSave>,
    queue: VecDeque<QueuedSave>,
}

struct QueuedSave {
    request_id: PersistenceRequestId,
    id: SaveId,
    kind: SaveKind,
    display_name: String,
    path: PathBuf,
    normalized_name: Option<String>,
    explicit: bool,
    requested_generation: u64,
    source: SnapshotSource,
}

struct RunningSave {
    request_id: PersistenceRequestId,
    id: SaveId,
    display_name: String,
    normalized_name: Option<String>,
    explicit: bool,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicBool>,
    handle: JoinHandle<Result<SaveJobOutcome, SaveJobError>>,
}

impl PendingSaveJobs {
    pub fn is_empty(&self) -> bool {
        self.running.is_none() && self.queue.is_empty()
    }

    pub fn any_running(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|job| !job.handle.is_finished())
    }

    /// Running (unfinished) plus queued ids in execution order, including
    /// same-target FIFO entries.
    pub fn pending_ids(&self) -> Vec<SaveId> {
        let mut ids = Vec::new();
        if let Some(running) = &self.running {
            ids.push(running.id.clone());
        }
        ids.extend(self.queue.iter().map(|queued| queued.id.clone()));
        ids
    }

    /// Whether the id has a running (including finished-but-not-reaped) or
    /// queued save. The poller reaps finished workers before admitting new
    /// requests each frame, so a completed target is immediately reusable.
    pub fn is_id_pending(&self, id: &SaveId) -> bool {
        if self.running.as_ref().is_some_and(|job| &job.id == id) {
            return true;
        }
        self.queue.iter().any(|queued| &queued.id == id)
    }

    pub fn is_name_pending(&self, normalized_name: &str) -> bool {
        if self
            .running
            .as_ref()
            .is_some_and(|job| job.normalized_name.as_deref() == Some(normalized_name))
        {
            return true;
        }
        self.queue
            .iter()
            .any(|queued| queued.normalized_name.as_deref() == Some(normalized_name))
    }

    /// Whether the normalized display name is held by a running or queued
    /// save for a *different* id. Same-id repeats are the same target and
    /// serialize FIFO; a different id claiming a pending name is a genuine
    /// collision and must wait.
    pub fn is_name_pending_by_other_id(&self, normalized_name: &str, id: &SaveId) -> bool {
        if self.running.as_ref().is_some_and(|job| {
            &job.id != id && job.normalized_name.as_deref() == Some(normalized_name)
        }) {
            return true;
        }
        self.queue.iter().any(|queued| {
            &queued.id != id && queued.normalized_name.as_deref() == Some(normalized_name)
        })
    }

    /// Number of queued (not yet started) save requests.
    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    /// Current progress phases for the running job and queued requests, in
    /// execution order. Frame code polls this without blocking the worker.
    pub fn progress(&self) -> Vec<(SaveId, SaveJobPhase)> {
        let mut phases = Vec::new();
        if let Some(running) = &self.running {
            phases.push((
                running.id.clone(),
                SaveJobPhase::decode(running.phase.load(Ordering::Relaxed)),
            ));
        }
        phases.extend(
            self.queue
                .iter()
                .map(|queued| (queued.id.clone(), SaveJobPhase::Queued)),
        );
        phases
    }

    /// Cancels queued saves with `id` and signals the running save when it
    /// matches. Queued cancellations never touch disk and are never reported
    /// as committed. A running save checks the flag before commit; once the
    /// commit point is reached the job runs to completion.
    pub fn cancel(&mut self, id: &SaveId) -> bool {
        let mut cancelled = false;
        let before = self.queue.len();
        self.queue.retain(|queued| &queued.id != id);
        cancelled |= self.queue.len() != before;
        if let Some(running) = &self.running
            && &running.id == id
            && !running.handle.is_finished()
        {
            running.cancel.store(true, Ordering::Relaxed);
            cancelled = true;
        }
        cancelled
    }

    fn queue_full(&self) -> bool {
        self.queue.len() >= MAX_QUEUED_SAVES
    }

    fn join_running(&mut self) {
        if let Some(job) = self.running.take()
            && let Err(error) = join_job(job.handle)
        {
            eprintln!("Cannot save {} during shutdown: {error}", job.display_name);
        }
    }

    fn start_next(&mut self) {
        const {
            assert!(MAX_SAVE_WORKERS >= 1 && MAX_RETAINED_SAVE_GENERATIONS >= 1);
        }
        if self.running.is_some() {
            return;
        }
        let Some(queued) = self.queue.pop_front() else {
            return;
        };
        let QueuedSave {
            request_id,
            id,
            kind,
            display_name,
            path,
            normalized_name,
            explicit,
            requested_generation,
            source,
        } = queued;
        let phase = Arc::new(AtomicU8::new(SaveJobPhase::Queued.encode()));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_phase = Arc::clone(&phase);
        let worker_cancel = Arc::clone(&cancel);
        let worker_id = id.clone();
        let worker_name = display_name.clone();
        let worker_kind = kind.clone();
        let worker_path = path.clone();
        let handle = thread::spawn(move || {
            run_save_worker(
                request_id,
                requested_generation,
                worker_id,
                worker_kind,
                worker_name,
                worker_path,
                source,
                worker_phase,
                worker_cancel,
            )
        });
        self.running = Some(RunningSave {
            request_id,
            id,
            display_name,
            normalized_name,
            explicit,
            phase,
            cancel,
            handle,
        });
    }
}

impl Drop for PendingSaveJobs {
    fn drop(&mut self) {
        // Queued requests never started, so dropping them cancels without
        // touching disk and without reporting a commit. Join the running
        // worker so an accepted save cannot be abandoned on teardown.
        self.queue.clear();
        self.join_running();
    }
}

pub(crate) struct CompletedJob {
    pub id: SaveId,
    pub display_name: String,
    pub explicit: bool,
    pub request_id: PersistenceRequestId,
    pub result: Result<SaveJobOutcome, SaveJobError>,
}

#[derive(Debug)]
pub(crate) struct SaveJobOutcome {
    pub request_id: PersistenceRequestId,
    pub requested_generation: u64,
    pub snapshot_world_generation: u64,
    pub snapshot_tick: u64,
    pub snapshot_lock_wait_ms: f64,
    pub snapshot_lock_hold_ms: f64,
    pub snapshot_capture_ms: f64,
    pub snapshot_blocked_fixed_ticks: u64,
    pub snapshot_wire_bytes: usize,
    pub serialize_ms: f64,
    pub write_ms: f64,
    pub total_ms: f64,
    pub bytes: usize,
}

#[allow(clippy::too_many_arguments)]
/// Enqueues a background save for the latest completed tick without blocking
/// on the simulation lock. Manual saves are FIFO (including same-target
/// overwrites in request order); autosaves coalesce by target and drop under
/// backpressure instead of queueing.
pub(crate) fn queue_save(
    id: SaveId,
    kind: SaveKind,
    display_name: String,
    path: PathBuf,
    normalized_name: Option<String>,
    explicit: bool,
    sim: &SimResource,
    pending: &mut PendingSaveJobs,
    status: &mut SaveLoadStatus,
    metrics: &mut SaveLoadMetrics,
) -> bool {
    // The frame poller reaps finished workers before admitting new requests,
    // so a completed target is reusable without losing its outcome here.
    // Autosaves coalesce: a running or queued autosave to the same target
    // already covers the request, so drop silently without an error status.
    if !explicit && pending.is_id_pending(&id) {
        return false;
    }
    if explicit
        && let Some(name) = normalized_name.as_deref()
        && pending.is_name_pending_by_other_id(name, &id)
    {
        status.message = Some(format!("{display_name} is already being saved."));
        status.kind = SaveLoadStatusKind::Info;
        return false;
    }
    if pending.queue_full() {
        if explicit {
            status.message =
                Some("Cannot save yet: another snapshot generation is still being saved.".into());
            status.kind = SaveLoadStatusKind::Info;
        }
        return false;
    }
    let submission_start = Instant::now();
    let requested_generation = sim.replacement_revision();
    let source = sim.snapshot_source();
    let request_id = PersistenceRequestId::next();
    pending.queue.push_back(QueuedSave {
        request_id,
        id: id.clone(),
        kind,
        display_name: display_name.clone(),
        path,
        normalized_name,
        explicit,
        requested_generation,
        source,
    });
    pending.start_next();
    metrics.last_request_submission_ms = submission_start.elapsed().as_secs_f64() * 1000.0;
    if explicit {
        status.message = Some(format!("Saving {display_name}..."));
        status.kind = SaveLoadStatusKind::Info;
        status.last_completed_id = None;
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn run_save_worker(
    request_id: PersistenceRequestId,
    requested_generation: u64,
    worker_id: SaveId,
    worker_kind: SaveKind,
    worker_name: String,
    worker_path: PathBuf,
    source: SnapshotSource,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicBool>,
) -> Result<SaveJobOutcome, SaveJobError> {
    let worker_start = Instant::now();
    phase.store(SaveJobPhase::Capturing.encode(), Ordering::Relaxed);
    // Background acquisition keeps frame submission responsive. Fixed ticks
    // defer via `try_write` while the read lock is held; frame readers stay
    // concurrent.
    let lock_wait_start = Instant::now();
    let sim = source
        .simulation
        .read()
        .map_err(|_| SaveJobError::LockPoisoned)?;
    let snapshot_lock_wait_ms = lock_wait_start.elapsed().as_secs_f64() * 1000.0;
    let lock_acquired = Instant::now();
    // Enforce the requested world identity: a world installed while this
    // request waited in the queue mutates the same lock, so capturing now
    // would mix the requested tick identity with another world's bytes. The
    // stale request is discarded with the previous save intact instead.
    let capture_generation = source.generation.load(Ordering::Acquire);
    if capture_generation != requested_generation {
        return Err(SaveJobError::Stale);
    }
    let capture_activity = SnapshotCaptureActivity::begin(&source.active_captures);
    let blocked_before = source.blocked_fixed_ticks.load(Ordering::Relaxed);
    if cancel.load(Ordering::Relaxed) {
        return Err(SaveJobError::Cancelled);
    }
    let snapshot_start = Instant::now();
    // Record-aware capture: partitioning decides what is saveable, not the
    // monolithic size pass.
    let snapshot =
        try_capture_record_snapshot(&sim, capture_generation).map_err(|error| match error {
            factory_sim::SaveLoadError::TooLarge => SaveJobError::CaptureBudget,
            _ => SaveJobError::CaptureFailed(format!("{error:?}")),
        })?;
    let snapshot_capture_ms = snapshot_start.elapsed().as_secs_f64() * 1000.0;
    let snapshot_identity = snapshot.identity();
    let snapshot_seed = snapshot.world_seed();
    drop(sim);
    drop(capture_activity);
    let snapshot_lock_hold_ms = lock_acquired.elapsed().as_secs_f64() * 1000.0;
    let snapshot_blocked_fixed_ticks = source
        .blocked_fixed_ticks
        .load(Ordering::Relaxed)
        .saturating_sub(blocked_before);
    if cancel.load(Ordering::Relaxed) {
        return Err(SaveJobError::Cancelled);
    }
    let metadata = SaveMetadata {
        schema_version: METADATA_SCHEMA_VERSION,
        id: worker_id,
        display_name: worker_name,
        kind: worker_kind,
        completed_at_unix_ms: now_unix_ms(),
        application_version: env!("CARGO_PKG_VERSION").into(),
        world_seed: Some(snapshot_seed),
    };
    phase.store(SaveJobPhase::Encoding.encode(), Ordering::Relaxed);
    // Cancellation is safe only before commit. Once the writer starts, the
    // job runs to its commit point; post-commit cleanup failures never roll
    // back a committed save (see `container`).
    if cancel.load(Ordering::Relaxed) {
        return Err(SaveJobError::Cancelled);
    }
    phase.store(SaveJobPhase::Writing.encode(), Ordering::Relaxed);
    let write_start = Instant::now();
    let stream = write_save_snapshot(&worker_path, &metadata, &snapshot)
        .map_err(|error| SaveJobError::Io(error.to_string()))?;
    phase.store(SaveJobPhase::Committing.encode(), Ordering::Relaxed);
    let stream_ms = write_start.elapsed().as_secs_f64() * 1000.0;
    // The captured world remains alive only until the streaming encoder is
    // finished; no complete encoded payload or container is retained.
    drop(snapshot);
    Ok(SaveJobOutcome {
        request_id,
        requested_generation,
        snapshot_world_generation: snapshot_identity.world_generation,
        snapshot_tick: snapshot_identity.tick,
        snapshot_lock_wait_ms,
        snapshot_lock_hold_ms,
        snapshot_capture_ms,
        snapshot_blocked_fixed_ticks,
        snapshot_wire_bytes: stream.simulation_bytes,
        serialize_ms: stream.encode_ms,
        write_ms: (stream_ms - stream.encode_ms).max(0.0),
        total_ms: worker_start.elapsed().as_secs_f64() * 1000.0,
        bytes: stream.total_bytes,
    })
}

pub(crate) fn take_completed(pending: &mut PendingSaveJobs) -> Vec<CompletedJob> {
    let mut completed = Vec::new();
    if let Some(job) = pending.running.take() {
        if job.handle.is_finished() {
            let result = join_job(job.handle);
            completed.push(CompletedJob {
                id: job.id,
                display_name: job.display_name,
                explicit: job.explicit,
                request_id: job.request_id,
                result,
            });
        } else {
            pending.running = Some(job);
        }
    }
    // Start the next FIFO entry (same-target writes serialize here in request
    // order) before returning, so throughput does not stall one poll behind.
    pending.start_next();
    completed
}

fn join_job(
    handle: JoinHandle<Result<SaveJobOutcome, SaveJobError>>,
) -> Result<SaveJobOutcome, SaveJobError> {
    handle
        .join()
        .unwrap_or_else(|_| Err(SaveJobError::Encode("save worker panicked".into())))
}

pub(crate) fn system_path(config: &SaveLoadConfig, kind: &SaveKind) -> PathBuf {
    match kind {
        SaveKind::Named => unreachable!("named saves use generated paths"),
        SaveKind::Quicksave => config.root_dir.join("quicksave.factsim"),
        SaveKind::Autosave { generation } => config
            .root_dir
            .join(format!("autosave-{generation}.factsim")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_running_save(id: &str) -> (RunningSave, std::sync::mpsc::SyncSender<()>) {
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
        let handle = thread::spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Err(SaveJobError::Encode("intentional test result".into()))
        });
        started_rx.recv().unwrap();
        let running = RunningSave {
            request_id: PersistenceRequestId::next(),
            id: SaveId::new(id),
            display_name: id.into(),
            normalized_name: Some(id.into()),
            explicit: true,
            phase: Arc::new(AtomicU8::new(SaveJobPhase::Capturing.encode())),
            cancel: Arc::new(AtomicBool::new(false)),
            handle,
        };
        (running, release_tx)
    }

    #[test]
    fn only_an_active_worker_consumes_generation_capacity() {
        let (running, release) = test_running_save("test");
        let mut pending = PendingSaveJobs {
            running: Some(running),
            queue: VecDeque::new(),
        };

        assert!(pending.any_running());
        assert!(pending.is_id_pending(&SaveId::new("test")));
        assert!(pending.is_name_pending("test"));
        assert_eq!(pending.pending_ids(), [SaveId::new("test")]);

        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while pending
            .running
            .as_ref()
            .is_some_and(|job| !job.handle.is_finished())
        {
            assert!(Instant::now() < deadline, "test worker did not finish");
            thread::yield_now();
        }

        assert!(!pending.any_running());
        assert!(pending.is_id_pending(&SaveId::new("test")));
        assert_eq!(take_completed(&mut pending).len(), 1);
        assert!(pending.is_empty());
    }

    #[test]
    fn queued_saves_stay_within_documented_bounds() {
        let sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        let source = sim.snapshot_source();
        let mut pending = PendingSaveJobs::default();
        for index in 0..MAX_QUEUED_SAVES {
            pending.queue.push_back(QueuedSave {
                request_id: PersistenceRequestId::next(),
                id: SaveId::new(format!("queued-{index}")),
                kind: SaveKind::Quicksave,
                display_name: format!("Queued {index}"),
                path: PathBuf::from(format!("queued-{index}.factsim")),
                normalized_name: None,
                explicit: true,
                requested_generation: 0,
                source: SnapshotSource {
                    simulation: Arc::clone(&source.simulation),
                    generation: Arc::clone(&source.generation),
                    active_captures: Arc::clone(&source.active_captures),
                    blocked_fixed_ticks: Arc::clone(&source.blocked_fixed_ticks),
                },
            });
        }
        assert_eq!(pending.queued_len(), MAX_QUEUED_SAVES);
        assert!(pending.queue_full());
        assert_eq!(
            pending.progress().len(),
            MAX_QUEUED_SAVES,
            "queued requests expose progress without retaining snapshots"
        );
    }

    #[test]
    fn cancel_removes_queued_and_signals_running() {
        let (running, release) = test_running_save("running");
        let sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        let source = sim.snapshot_source();
        let mut pending = PendingSaveJobs {
            running: Some(running),
            queue: VecDeque::from([QueuedSave {
                request_id: PersistenceRequestId::next(),
                id: SaveId::new("queued"),
                kind: SaveKind::Quicksave,
                display_name: "Queued".into(),
                path: PathBuf::from("queued.factsim"),
                normalized_name: None,
                explicit: true,
                requested_generation: 0,
                source,
            }]),
        };
        assert!(pending.cancel(&SaveId::new("queued")));
        assert!(pending.queue.is_empty());
        assert!(pending.cancel(&SaveId::new("running")));
        assert!(
            pending
                .running
                .as_ref()
                .is_some_and(|job| job.cancel.load(Ordering::Relaxed)),
            "running cancellation must signal the worker before commit"
        );
        release.send(()).unwrap();
        pending.join_running();
    }
}
