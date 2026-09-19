use super::catalog::now_unix_ms;
use super::container::{ContainerError, METADATA_SCHEMA_VERSION, write_save_snapshot};
use super::lifecycle::{
    MAX_QUEUED_SAVES, MAX_RETAINED_SAVE_GENERATIONS, MAX_SAVE_WORKERS, PersistenceRequestId,
    SAVE_CANCEL_ACTIVE, SAVE_CANCEL_COMMITTING, SAVE_CANCEL_REQUESTED, SaveJobError, SaveJobPhase,
};
use super::{
    SaveId, SaveKind, SaveLoadConfig, SaveLoadMetrics, SaveLoadStatus, SaveLoadStatusKind,
    SaveMetadata,
};
use crate::resources::{AdmissionCaptureClaim, AdmissionCaptureSource, SimResource};
use factory_sim::{SimulationSaveSnapshot, try_capture_record_snapshot};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Snapshot secured by an admission capture task, parked for the worker to
/// pick up. The worker never touches the simulation lock: by the time it
/// runs, the snapshot below is already immutable.
struct ParkedSnapshot {
    snapshot: SimulationSaveSnapshot,
    snapshot_lock_wait_ms: f64,
    snapshot_lock_hold_ms: f64,
    snapshot_capture_ms: f64,
    snapshot_blocked_fixed_ticks: u64,
}

/// Bounded asynchronous save queue.
///
/// At most [`MAX_SAVE_WORKERS`] worker runs at a time and retains one
/// immutable snapshot generation. Accepted manual saves wait in a bounded
/// FIFO ([`MAX_QUEUED_SAVES`]); autosaves coalesce by target and apply
/// backpressure by dropping instead of queueing. Submission never blocks on
/// the simulation lock: an admission capture task secures the snapshot under
/// the simulation read lock in the background, so frame schedules stay
/// responsive while capture, encoding, and disk I/O proceed.
///
/// Each admission spawns its own capture task immediately, so the
/// tick-deferral window covers only admission-to-capture: fixed ticks defer
/// while any capture is in flight, then resume once every admitted snapshot
/// is parked. Queued entries and running workers hold already-secured
/// snapshots, never the simulation lock.
///
/// Queued entries hold only request parameters plus the parked-snapshot
/// channel; snapshot memory is retained solely by the parked handoff until
/// the worker encodes it.
#[derive(bevy::prelude::Resource, Default)]
pub struct PendingSaveJobs {
    running: Option<RunningSave>,
    queue: VecDeque<QueuedSave>,
    /// Teardown signal shared with the running worker. Set on drop so a
    /// worker waiting on the artifact lock — possibly held by a detached
    /// scan worker — aborts instead of stalling shutdown through it. A
    /// worker already past acquisition still runs to its commit point.
    shutdown: Arc<AtomicBool>,
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
    requested_tick: u64,
    /// Created at admission so a queued cancel reaches the capture task
    /// before it secures the snapshot; moved into the worker at dequeue.
    cancel: Arc<AtomicU8>,
    /// Handoff from the admission capture task. The worker blocks here only
    /// until its snapshot is parked — the simulation lock is never held
    /// across this wait. The mutex exists solely for the Bevy resource
    /// `Sync` bound while the entry sits in the queue; after dequeue the
    /// worker is the single owner and unwraps it without contention.
    parked: Mutex<mpsc::Receiver<Result<ParkedSnapshot, SaveJobError>>>,
}

struct RunningSave {
    request_id: PersistenceRequestId,
    id: SaveId,
    kind: SaveKind,
    display_name: String,
    path: PathBuf,
    normalized_name: Option<String>,
    explicit: bool,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicU8>,
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
    /// as committed. A running save honors the request at the atomic commit
    /// point inside the writer: the cancellation and the worker's commit
    /// claim race on a single compare-exchange, so either the cancel wins
    /// (no commit) or the worker already claimed committing (cancel reports
    /// false — too late — instead of a false success).
    pub fn cancel(&mut self, id: &SaveId) -> bool {
        let mut cancelled = false;
        // Signal queued entries first: their capture task may still be
        // running and checks this flag before securing the snapshot, so a
        // queued cancel avoids wasted capture work. The entries are dropped
        // below regardless; a task that already parked finds its receiver
        // gone and drops the snapshot instead of retaining it.
        for queued in &self.queue {
            if &queued.id == id {
                queued
                    .cancel
                    .store(SAVE_CANCEL_REQUESTED, Ordering::Relaxed);
            }
        }
        let before = self.queue.len();
        self.queue.retain(|queued| &queued.id != id);
        cancelled |= self.queue.len() != before;
        if let Some(running) = &self.running
            && &running.id == id
            && !running.handle.is_finished()
        {
            // ACTIVE -> REQUESTED wins; REQUESTED (already asked) still
            // counts; COMMITTING means the worker already claimed the
            // commit point and the cancel is too late.
            match running.cancel.compare_exchange(
                SAVE_CANCEL_ACTIVE,
                SAVE_CANCEL_REQUESTED,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => cancelled = true,
                Err(SAVE_CANCEL_REQUESTED) => cancelled = true,
                Err(SAVE_CANCEL_COMMITTING) => {}
                Err(_) => {}
            }
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
            requested_tick,
            cancel,
            parked,
        } = queued;
        let phase = Arc::new(AtomicU8::new(SaveJobPhase::Queued.encode()));
        let worker_phase = Arc::clone(&phase);
        let worker_cancel = Arc::clone(&cancel);
        let worker_id = id.clone();
        let worker_name = display_name.clone();
        let worker_kind = kind.clone();
        let worker_path = path.clone();
        let worker_shutdown = Arc::clone(&self.shutdown);
        let handle = thread::spawn(move || {
            run_save_worker(
                request_id,
                requested_generation,
                requested_tick,
                worker_id,
                worker_kind,
                worker_name,
                worker_path,
                parked,
                worker_phase,
                worker_cancel,
                worker_shutdown,
            )
        });
        self.running = Some(RunningSave {
            request_id,
            id,
            kind,
            display_name,
            path,
            normalized_name,
            explicit,
            phase,
            cancel,
            handle,
        });
    }
}

/// How long teardown waits for a running save to finish before admitting
/// shutdown. Absorbs normal artifact-lock holds (scan recovery, a commit
/// landing concurrently) so an accepted save still completes whenever the
/// lock frees promptly, while capping the stall when the holder is a
/// detached scan worker on a pathological filesystem.
const SHUTDOWN_DRAIN_GRACE: Duration = Duration::from_secs(5);

impl Drop for PendingSaveJobs {
    fn drop(&mut self) {
        // Queued requests never started, so dropping them cancels without
        // touching disk and without reporting a commit. Drain briefly so a
        // running save finishes when the artifact lock frees promptly,
        // then signal teardown so a worker still waiting on the lock
        // (possibly held by a detached scan worker) aborts instead of
        // stalling shutdown through it. Joining afterwards still lets an
        // accepted save already past acquisition run to completion.
        // Never starts new workers here: `take_completed` would.
        let deadline = Instant::now() + SHUTDOWN_DRAIN_GRACE;
        while self
            .running
            .as_ref()
            .is_some_and(|job| !job.handle.is_finished())
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
        self.shutdown.store(true, Ordering::Relaxed);
        self.queue.clear();
        self.join_running();
    }
}

pub(crate) struct CompletedJob {
    pub id: SaveId,
    pub kind: SaveKind,
    pub display_name: String,
    pub path: PathBuf,
    pub explicit: bool,
    pub request_id: PersistenceRequestId,
    pub result: Result<SaveJobOutcome, SaveJobError>,
}

#[derive(Debug)]
pub(crate) struct SaveJobOutcome {
    pub request_id: PersistenceRequestId,
    pub requested_generation: u64,
    pub requested_tick: u64,
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
    // Claim before reading the admission tick so fixed ticks defer across
    // the whole admission-to-capture window. The guard moves into the
    // capture task and releases when the snapshot is parked (or the task
    // panics), so the deferral never outlives the capture.
    let claim = sim.claim_admission_capture();
    // The completed tick at admission, read lock-free so submission never
    // blocks on the simulation lock. The deferral above holds the world on
    // exactly this tick until the capture task secures its snapshot, so the
    // worker captures this identity without ever touching the simulation
    // lock; the captured tick is reported in the outcome.
    let requested_tick = sim.completed_tick();
    let source = sim.capture_source();
    let cancel = Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE));
    let worker_cancel = Arc::clone(&cancel);
    let (parked_tx, parked_rx) = mpsc::channel();
    thread::spawn(move || {
        admission_capture_task(
            requested_generation,
            source,
            worker_cancel,
            claim,
            parked_tx,
        );
    });
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
        requested_tick,
        cancel,
        parked: Mutex::new(parked_rx),
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

/// Secures one admitted snapshot under the simulation read lock and parks
/// it for the worker. Spawned per admission — not at dequeue — so the
/// tick-deferral window covers only this capture, never queue wait or disk
/// I/O. The claim guard releases the deferral when this returns (or if the
/// task panics); a disconnected channel therefore tells the worker its
/// capture task died. A parked result whose receiver is gone (queued cancel,
/// teardown) is dropped here instead of retained.
fn admission_capture_task(
    requested_generation: u64,
    source: AdmissionCaptureSource,
    cancel: Arc<AtomicU8>,
    _claim: AdmissionCaptureClaim,
    parked: mpsc::Sender<Result<ParkedSnapshot, SaveJobError>>,
) {
    let outcome = capture_admission(requested_generation, &source, &cancel);
    let _ = parked.send(outcome);
}

fn capture_admission(
    requested_generation: u64,
    source: &AdmissionCaptureSource,
    cancel: &AtomicU8,
) -> Result<ParkedSnapshot, SaveJobError> {
    if cancel.load(Ordering::Relaxed) != SAVE_CANCEL_ACTIVE {
        return Err(SaveJobError::Cancelled);
    }
    // Background acquisition keeps frame submission responsive. Fixed ticks
    // defer while this capture is in flight; frame readers stay concurrent.
    let lock_wait_start = Instant::now();
    let sim = source
        .simulation
        .read()
        .map_err(|_| SaveJobError::LockPoisoned)?;
    let snapshot_lock_wait_ms = lock_wait_start.elapsed().as_secs_f64() * 1000.0;
    let lock_acquired = Instant::now();
    // Enforce the requested world identity: a world installed while this
    // request waited mutates the same lock, so capturing now would mix the
    // requested tick identity with another world's bytes. The stale request
    // is discarded with the previous save intact instead. Tick identity
    // needs no check here: fixed ticks defer while any admission capture is
    // in flight, so the world cannot advance between admission and this
    // capture. `requested_tick` is retained for observability and the
    // captured tick is reported in the outcome.
    let capture_generation = source.generation.load(Ordering::Acquire);
    if capture_generation != requested_generation {
        return Err(SaveJobError::Stale);
    }
    let blocked_before = source.blocked_fixed_ticks.load(Ordering::Relaxed);
    if cancel.load(Ordering::Relaxed) != SAVE_CANCEL_ACTIVE {
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
    drop(sim);
    let snapshot_lock_hold_ms = lock_acquired.elapsed().as_secs_f64() * 1000.0;
    let snapshot_blocked_fixed_ticks = source
        .blocked_fixed_ticks
        .load(Ordering::Relaxed)
        .saturating_sub(blocked_before);
    if cancel.load(Ordering::Relaxed) != SAVE_CANCEL_ACTIVE {
        return Err(SaveJobError::Cancelled);
    }
    Ok(ParkedSnapshot {
        snapshot,
        snapshot_lock_wait_ms,
        snapshot_lock_hold_ms,
        snapshot_capture_ms,
        snapshot_blocked_fixed_ticks,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_save_worker(
    request_id: PersistenceRequestId,
    requested_generation: u64,
    requested_tick: u64,
    worker_id: SaveId,
    worker_kind: SaveKind,
    worker_name: String,
    worker_path: PathBuf,
    parked: Mutex<mpsc::Receiver<Result<ParkedSnapshot, SaveJobError>>>,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicU8>,
    shutdown: Arc<AtomicBool>,
) -> Result<SaveJobOutcome, SaveJobError> {
    let worker_start = Instant::now();
    phase.store(SaveJobPhase::Capturing.encode(), Ordering::Relaxed);
    // Single owner after dequeue: the mutex was never locked while queued,
    // so unwrapping cannot observe poisoning.
    let parked = parked
        .into_inner()
        .expect("parked receiver is never shared before handoff");
    // The admission capture task secured the snapshot under the simulation
    // read lock at submission time; the worker only picks it up here, so it
    // never touches the simulation lock and fixed ticks already resumed once
    // the capture parked. A disconnected channel means the capture task
    // panicked. World identity was enforced at capture; the deferral between
    // admission and capture preserves the requested tick identity, so
    // neither needs re-checking here — `requested_tick` is retained for
    // observability and the captured tick is reported in the outcome.
    let ParkedSnapshot {
        snapshot,
        snapshot_lock_wait_ms,
        snapshot_lock_hold_ms,
        snapshot_capture_ms,
        snapshot_blocked_fixed_ticks,
    } = parked
        .recv()
        .map_err(|_| SaveJobError::Encode("admission capture task panicked".into()))??;
    let snapshot_identity = snapshot.identity();
    let snapshot_seed = snapshot.world_seed();
    if cancel.load(Ordering::Relaxed) != SAVE_CANCEL_ACTIVE {
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
    // A pre-write cancellation avoids starting the writer; a cancel racing
    // the encode loses or wins the atomic commit claim inside it. Once the
    // worker claims committing, the job runs to completion, and post-commit
    // cleanup failures never roll back a committed save (see `container`).
    if cancel.load(Ordering::Relaxed) != SAVE_CANCEL_ACTIVE {
        return Err(SaveJobError::Cancelled);
    }
    phase.store(SaveJobPhase::Writing.encode(), Ordering::Relaxed);
    let write_start = Instant::now();
    // Shutdown admission before the artifact lock: a detached scan worker
    // may hold it across recovery, and teardown joins this worker — so an
    // unconditional wait would stall shutdown indirectly through the
    // detached holder. Abandoning the wait reports cancellation; nothing
    // has committed yet.
    let Some(write) = write_save_snapshot(&worker_path, &metadata, &snapshot, &shutdown, &cancel)
    else {
        return Err(SaveJobError::Cancelled);
    };
    // A cancel racing the encode is honored at the commit point inside
    // the writer and surfaces here instead of reporting success.
    let stream = write.map_err(|error| match error {
        ContainerError::Cancelled => SaveJobError::Cancelled,
        error => SaveJobError::Io(error.to_string()),
    })?;
    phase.store(SaveJobPhase::Committing.encode(), Ordering::Relaxed);
    let stream_ms = write_start.elapsed().as_secs_f64() * 1000.0;
    // The captured world remains alive only until the streaming encoder is
    // finished; no complete encoded payload or container is retained.
    drop(snapshot);
    Ok(SaveJobOutcome {
        request_id,
        requested_generation,
        requested_tick,
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
                kind: job.kind,
                display_name: job.display_name,
                path: job.path,
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
            kind: SaveKind::Quicksave,
            display_name: id.into(),
            path: PathBuf::from("test.factsim"),
            normalized_name: Some(id.into()),
            explicit: true,
            phase: Arc::new(AtomicU8::new(SaveJobPhase::Capturing.encode())),
            cancel: Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE)),
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
            shutdown: Arc::new(AtomicBool::new(false)),
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

    fn test_queued_save(id: &str, requested_tick: u64) -> QueuedSave {
        // The sender is dropped: these queue-shape tests never run a
        // capture task, they only assert on queue bookkeeping.
        let (_, parked) = mpsc::channel();
        QueuedSave {
            request_id: PersistenceRequestId::next(),
            id: SaveId::new(id),
            kind: SaveKind::Quicksave,
            display_name: id.into(),
            path: PathBuf::from(format!("{id}.factsim")),
            normalized_name: None,
            explicit: true,
            requested_generation: 0,
            requested_tick,
            cancel: Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE)),
            parked: Mutex::new(parked),
        }
    }

    #[test]
    fn admission_captures_defer_fixed_ticks_until_parked() {
        use crate::resources::{FixedStepCatchUpStats, SimProfileStats, UpsStats};
        use crate::simulation::{SimCommandBacklog, SimCommandResult, tick_sim};
        use bevy::prelude::{App, Update};

        let mut app = App::new();
        app.insert_resource(SimResource::new(factory_sim::Simulation::new_test_world(7)))
            .init_resource::<SimCommandBacklog>()
            .init_resource::<SimProfileStats>()
            .init_resource::<FixedStepCatchUpStats>()
            .init_resource::<UpsStats>()
            .add_message::<SimCommandResult>()
            .add_systems(Update, tick_sim);

        let tick = |app: &App| app.world().resource::<SimResource>().read().tick_count();
        let blocked = |app: &App| {
            app.world()
                .resource::<SimProfileStats>()
                .save_blocked_fixed_ticks
        };
        let before = tick(&app);
        // Idle: ticks advance, nothing blocked.
        app.update();
        assert_eq!(tick(&app), before + 1);
        assert_eq!(blocked(&app), 0);

        // A held admission claim defers fixed steps so the world cannot tick
        // out from under the requested completed-tick identity before the
        // capture task secures its snapshot.
        let claim = app
            .world()
            .resource::<SimResource>()
            .claim_admission_capture();
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            tick(&app),
            before + 1,
            "ticks must defer while an admission capture is in flight"
        );
        assert_eq!(blocked(&app), 3);

        // Released claim (snapshot parked): ticks resume on the very next
        // step while encoding and disk I/O would still proceed elsewhere.
        drop(claim);
        app.update();
        assert_eq!(tick(&app), before + 2);
        assert_eq!(blocked(&app), 3);
    }

    #[test]
    fn admission_capture_parks_requested_tick_identity() {
        let sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        let requested_generation = sim.replacement_revision();
        let requested_tick = sim.completed_tick();
        let source = sim.capture_source();
        let cancel = AtomicU8::new(SAVE_CANCEL_ACTIVE);

        let parked = capture_admission(requested_generation, &source, &cancel)
            .expect("admission capture should park the requested tick");
        assert_eq!(
            parked.snapshot.identity().tick,
            requested_tick,
            "the parked snapshot must carry the admission tick identity"
        );

        // A world installed after admission is stale, never mixed into the
        // requested identity.
        let stale = capture_admission(requested_generation.wrapping_add(1), &source, &cancel);
        assert!(
            matches!(stale, Err(SaveJobError::Stale)),
            "a post-admission world install must be discarded as stale"
        );

        // A cancel racing the capture wins before any snapshot is retained.
        cancel.store(SAVE_CANCEL_REQUESTED, Ordering::Relaxed);
        let cancelled = capture_admission(requested_generation, &source, &cancel);
        assert!(
            matches!(cancelled, Err(SaveJobError::Cancelled)),
            "a pre-capture cancel must win without retaining a snapshot"
        );
    }

    #[test]
    fn queued_saves_stay_within_documented_bounds() {
        let sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        let requested_tick = sim.completed_tick();
        let mut pending = PendingSaveJobs::default();
        for index in 0..MAX_QUEUED_SAVES {
            pending
                .queue
                .push_back(test_queued_save(&format!("queued-{index}"), requested_tick));
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
        let queued = test_queued_save("queued", sim.completed_tick());
        let queued_cancel = Arc::clone(&queued.cancel);
        let mut pending = PendingSaveJobs {
            running: Some(running),
            shutdown: Arc::new(AtomicBool::new(false)),
            queue: VecDeque::from([queued]),
        };
        assert!(pending.cancel(&SaveId::new("queued")));
        assert!(pending.queue.is_empty());
        assert_eq!(
            queued_cancel.load(Ordering::Relaxed),
            SAVE_CANCEL_REQUESTED,
            "a queued cancel must reach the capture task before the entry is dropped"
        );
        assert!(pending.cancel(&SaveId::new("running")));
        assert!(
            pending
                .running
                .as_ref()
                .is_some_and(|job| job.cancel.load(Ordering::Relaxed) == SAVE_CANCEL_REQUESTED),
            "running cancellation must signal the worker before commit"
        );
        release.send(()).unwrap();
        pending.join_running();
    }

    #[test]
    fn cancel_after_commit_claim_reports_too_late() {
        // The worker claims ACTIVE -> COMMITTING with a single
        // compare-exchange before the rename. A cancel arriving afterwards
        // must report false (too late) instead of a false success for a
        // save that is about to commit.
        let (running, release) = test_running_save("committing");
        running
            .cancel
            .store(SAVE_CANCEL_COMMITTING, Ordering::Relaxed);
        let mut pending = PendingSaveJobs {
            running: Some(running),
            queue: VecDeque::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
        };
        assert!(
            !pending.cancel(&SaveId::new("committing")),
            "cancel after the commit claim must report too late, not success"
        );
        // The flag stays COMMITTING: the worker owns the commit point.
        assert_eq!(
            pending
                .running
                .as_ref()
                .map(|job| job.cancel.load(Ordering::Relaxed)),
            Some(SAVE_CANCEL_COMMITTING)
        );
        release.send(()).unwrap();
        pending.join_running();
    }

    #[test]
    fn shutdown_releases_save_waiting_on_artifact_lock() {
        // A detached scan worker holds the artifact lock across recovery.
        let _held = crate::save_load::container::hold_save_artifact_lock_for_tests();
        let sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        let source = sim.capture_source();
        let requested = source.generation.load(Ordering::Acquire);
        let requested_tick = sim.completed_tick();
        let phase = Arc::new(AtomicU8::new(SaveJobPhase::Queued.encode()));
        let worker_phase = Arc::clone(&phase);
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        // Park the admission synchronously: the worker under test starts at
        // the channel handoff, already past snapshot capture.
        let (parked_tx, parked_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE));
        admission_capture_task(
            requested,
            source,
            Arc::clone(&cancel),
            sim.claim_admission_capture(),
            parked_tx,
        );
        let handle = thread::spawn(move || {
            run_save_worker(
                PersistenceRequestId::next(),
                requested,
                requested_tick,
                SaveId::new("shutdown-probe"),
                SaveKind::Quicksave,
                "Shutdown probe".into(),
                std::env::temp_dir().join("factory-shutdown-probe.factsim"),
                Mutex::new(parked_rx),
                worker_phase,
                cancel,
                worker_shutdown,
            )
        });
        // Wait until the worker blocks in the artifact-lock wait.
        let deadline = Instant::now() + Duration::from_secs(10);
        while SaveJobPhase::decode(phase.load(Ordering::Relaxed)) != SaveJobPhase::Writing
            && !handle.is_finished()
        {
            assert!(
                Instant::now() < deadline,
                "worker never reached the artifact-lock wait"
            );
            thread::yield_now();
        }
        assert_eq!(
            SaveJobPhase::decode(phase.load(Ordering::Relaxed)),
            SaveJobPhase::Writing,
            "worker finished before blocking on the artifact lock"
        );
        // Teardown admission releases the wait as cancellation instead of
        // joining shutdown through the detached holder.
        shutdown.store(true, Ordering::Relaxed);
        let result = handle.join().expect("save worker panicked");
        assert!(
            matches!(result, Err(SaveJobError::Cancelled)),
            "expected cancellation, got {result:?}"
        );
    }

    #[test]
    fn drop_signals_shutdown_before_joining() {
        let mut pending = PendingSaveJobs::default();
        let observed = Arc::new(AtomicBool::new(false));
        let worker_observed = Arc::clone(&observed);
        let worker_shutdown = Arc::clone(&pending.shutdown);
        let handle = thread::spawn(move || {
            // Outlives the drain grace: the flag is set only after it.
            let deadline = Instant::now() + SHUTDOWN_DRAIN_GRACE + Duration::from_secs(5);
            while !worker_shutdown.load(Ordering::Relaxed) {
                assert!(Instant::now() < deadline, "drop did not signal shutdown");
                thread::yield_now();
            }
            worker_observed.store(true, Ordering::Relaxed);
            Err(SaveJobError::Encode("teardown".into()))
        });
        pending.running = Some(RunningSave {
            request_id: PersistenceRequestId::next(),
            id: SaveId::new("teardown"),
            kind: SaveKind::Quicksave,
            display_name: "Teardown".into(),
            path: PathBuf::from("teardown.factsim"),
            normalized_name: None,
            explicit: true,
            phase: Arc::new(AtomicU8::new(SaveJobPhase::Capturing.encode())),
            cancel: Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE)),
            handle,
        });
        let start = Instant::now();
        drop(pending);
        assert!(
            start.elapsed() < SHUTDOWN_DRAIN_GRACE + Duration::from_secs(5),
            "drop stalled instead of draining, signalling shutdown, and joining"
        );
        assert!(
            observed.load(Ordering::Relaxed),
            "running worker was not released by the shutdown signal"
        );
    }

    #[test]
    fn mid_encode_cancel_reports_cancelled_without_committing() {
        // Large world so the encode spans many frames; cancel lands
        // mid-encode, long before the commit.
        let mut sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        {
            let mut guard = sim.write_for_tests();
            for y in -20..20 {
                for x in -20..20 {
                    guard.ensure_chunk_generated(factory_sim::ChunkCoord { x, y });
                }
            }
        }
        let source = sim.capture_source();
        let requested = source.generation.load(Ordering::Acquire);
        let requested_tick = sim.completed_tick();
        let phase = Arc::new(AtomicU8::new(SaveJobPhase::Queued.encode()));
        let worker_phase = Arc::clone(&phase);
        let cancel = Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE));
        let worker_cancel = Arc::clone(&cancel);
        let root =
            std::env::temp_dir().join(format!("factory-mid-encode-cancel-{}", std::process::id()));
        let worker_path = root.join("probe.factsim");
        let committed_path = worker_path.clone();
        // Park the admission synchronously: the worker under test starts at
        // the channel handoff, already past snapshot capture.
        let (parked_tx, parked_rx) = mpsc::channel();
        admission_capture_task(
            requested,
            source,
            Arc::clone(&worker_cancel),
            sim.claim_admission_capture(),
            parked_tx,
        );
        let handle = thread::spawn(move || {
            run_save_worker(
                PersistenceRequestId::next(),
                requested,
                requested_tick,
                SaveId::new("probe"),
                SaveKind::Quicksave,
                "Probe".into(),
                worker_path,
                Mutex::new(parked_rx),
                worker_phase,
                worker_cancel,
                Arc::new(AtomicBool::new(false)),
            )
        });
        // Wait until the worker is encoding/writing, then cancel: the
        // request is provably still in flight, before any commit.
        let deadline = Instant::now() + Duration::from_secs(15);
        while SaveJobPhase::decode(phase.load(Ordering::Relaxed)) != SaveJobPhase::Writing
            && !handle.is_finished()
        {
            assert!(Instant::now() < deadline, "worker never reached the encode");
            thread::yield_now();
        }
        assert_eq!(
            SaveJobPhase::decode(phase.load(Ordering::Relaxed)),
            SaveJobPhase::Writing,
            "worker finished before the cancel could land mid-encode"
        );
        cancel.store(SAVE_CANCEL_REQUESTED, Ordering::Relaxed);
        let result = handle.join().expect("save worker panicked");
        assert!(
            matches!(result, Err(SaveJobError::Cancelled)),
            "mid-encode cancel still committed: {result:?}"
        );
        assert!(
            !committed_path.exists(),
            "a cancelled save must not install its target"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn queued_save_preserves_admission_tick_when_world_advances_while_waiting() {
        // A save admitted at tick N parks its snapshot at admission; when
        // the same world advances to N+1 while the entry waits, the worker
        // still commits tick N — the requested completed-tick identity —
        // because fixed ticks deferred only across admission-to-capture. The
        // requested tick is retained for observability and the captured tick
        // is reported in the outcome.
        let mut sim = SimResource::new(factory_sim::Simulation::new_test_world(7));
        let requested_generation = sim.replacement_revision();
        let requested_tick = sim.completed_tick();
        // Park the admission, then advance the world while the entry
        // "waits" for the worker.
        let (parked_tx, parked_rx) = mpsc::channel();
        admission_capture_task(
            requested_generation,
            sim.capture_source(),
            Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE)),
            sim.claim_admission_capture(),
            parked_tx,
        );
        sim.write_for_tests().tick();
        sim.publish_completed_tick(sim.read().tick_count());
        let live_tick = sim.read().tick_count();
        assert_ne!(
            live_tick, requested_tick,
            "the test must advance the tick after admission"
        );
        let root = std::env::temp_dir().join(format!(
            "factory-tick-fifo-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let outcome = run_save_worker(
            PersistenceRequestId::next(),
            requested_generation,
            requested_tick,
            SaveId::new("tick-fifo"),
            SaveKind::Quicksave,
            "Tick fifo".into(),
            root.join("probe.factsim"),
            Mutex::new(parked_rx),
            Arc::new(AtomicU8::new(SaveJobPhase::Queued.encode())),
            Arc::new(AtomicU8::new(SAVE_CANCEL_ACTIVE)),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("a queued save must commit its parked admission tick");
        assert_eq!(
            outcome.snapshot_tick, requested_tick,
            "the worker must commit the admission tick, not the latest tick"
        );
        assert_eq!(
            outcome.requested_tick, requested_tick,
            "the outcome must retain the admission tick for observability"
        );
        assert!(
            root.join("probe.factsim").exists(),
            "the queued save must commit its target"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
