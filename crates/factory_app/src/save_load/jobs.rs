use super::catalog::now_unix_ms;
use super::container::{METADATA_SCHEMA_VERSION, encode_container, write_save_bytes};
use super::{
    SaveId, SaveKind, SaveLoadConfig, SaveLoadMetrics, SaveLoadStatus, SaveLoadStatusKind,
    SaveMetadata,
};
use crate::resources::SimResource;
use factory_sim::{save_snapshot_to_bytes, try_capture_save_snapshot};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

/// The full-copy architecture retains at most one immutable world generation.
/// This is also a bound on background encoders and their temporary buffers.
const MAX_RETAINED_SAVE_GENERATIONS: usize = 1;

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

#[derive(bevy::prelude::Resource, Default)]
pub struct PendingSaveJobs {
    jobs: Vec<SaveJob>,
}

impl PendingSaveJobs {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
    pub fn any_running(&self) -> bool {
        self.jobs.iter().any(|job| !job.handle.is_finished())
    }
    pub fn is_id_pending(&self, id: &SaveId) -> bool {
        self.jobs.iter().any(|job| &job.id == id)
    }
    pub fn is_name_pending(&self, normalized_name: &str) -> bool {
        self.jobs
            .iter()
            .any(|job| job.normalized_name.as_deref() == Some(normalized_name))
    }
    pub fn pending_ids(&self) -> Vec<SaveId> {
        self.jobs.iter().map(|job| job.id.clone()).collect()
    }

    fn is_at_capacity(&self) -> bool {
        self.jobs
            .iter()
            .filter(|job| !job.handle.is_finished())
            .count()
            >= MAX_RETAINED_SAVE_GENERATIONS
    }

    fn join_all(&mut self) {
        for job in self.jobs.drain(..) {
            if let Err(error) = join_job(job.handle) {
                eprintln!("Cannot save {} during shutdown: {error}", job.display_name);
            }
        }
    }
}

impl Drop for PendingSaveJobs {
    fn drop(&mut self) {
        // Dropping a JoinHandle detaches its worker. Explicitly join every
        // accepted request so application teardown cannot abandon a first save.
        self.join_all();
    }
}

struct SaveJob {
    id: SaveId,
    display_name: String,
    normalized_name: Option<String>,
    explicit: bool,
    handle: JoinHandle<Result<SaveJobOutcome, String>>,
}

pub(crate) struct CompletedJob {
    pub id: SaveId,
    pub display_name: String,
    pub explicit: bool,
    pub result: Result<SaveJobOutcome, String>,
}

#[derive(Debug)]
pub(crate) struct SaveJobOutcome {
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
/// Starts a background worker that captures and saves the latest completed tick.
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
    if pending.is_id_pending(&id)
        || normalized_name
            .as_deref()
            .is_some_and(|name| pending.is_name_pending(name))
    {
        if explicit {
            status.message = Some(format!("{display_name} is already being saved."));
            status.kind = SaveLoadStatusKind::Info;
        }
        return false;
    }
    if pending.is_at_capacity() {
        if explicit {
            status.message =
                Some("Cannot save yet: another snapshot generation is still being saved.".into());
            status.kind = SaveLoadStatusKind::Info;
        }
        return false;
    }
    let submission_start = Instant::now();
    let source = sim.snapshot_source();
    let worker_id = id.clone();
    let worker_name = display_name.clone();
    let (capture_started_tx, capture_started_rx) = mpsc::sync_channel(0);
    let handle = thread::spawn(move || {
        let worker_start = Instant::now();
        // Signal only after acquiring the read lock. The request system runs
        // after FixedUpdate, so this pins its exact completed-tick boundary
        // without cloning the durable world on the main thread. Frame-side
        // readers remain concurrent; fixed ticks defer via `try_write`.
        let lock_wait_start = Instant::now();
        let sim = source
            .simulation
            .read()
            .map_err(|_| "simulation lock poisoned".to_string())?;
        let snapshot_lock_wait_ms = lock_wait_start.elapsed().as_secs_f64() * 1000.0;
        let lock_acquired = Instant::now();
        let capture_activity = SnapshotCaptureActivity::begin(&source.active_captures);
        let blocked_before = source.blocked_fixed_ticks.load(Ordering::Relaxed);
        capture_started_tx
            .send(())
            .map_err(|_| "save request was abandoned before capture".to_string())?;
        let snapshot_start = Instant::now();
        let snapshot = try_capture_save_snapshot(&sim, source.world_generation).map_err(
            |error| match error {
                factory_sim::SaveLoadError::TooLarge => {
                    "save exceeds this build's snapshot capture budget".into()
                }
                _ => format!("snapshot capture failed: {error:?}"),
            },
        )?;
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
        let serialize_start = Instant::now();
        let payload = save_snapshot_to_bytes(&snapshot).map_err(|error| match error {
            factory_sim::SaveLoadError::TooLarge => {
                "save exceeds this build's save size or collection limits".into()
            }
            _ => format!("simulation serialization failed: {error:?}"),
        })?;
        let serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;
        let snapshot_wire_bytes = payload.len();
        // Release the world copy before allocating the enclosing file buffer.
        drop(snapshot);
        let metadata = SaveMetadata {
            schema_version: METADATA_SCHEMA_VERSION,
            id: worker_id,
            display_name: worker_name,
            kind,
            completed_at_unix_ms: now_unix_ms(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            world_seed: Some(snapshot_seed),
        };
        let bytes = encode_container(&metadata, &payload).map_err(|error| error.to_string())?;
        drop(payload);
        let write_start = Instant::now();
        write_save_bytes(&path, &bytes).map_err(|error| error.to_string())?;
        Ok(SaveJobOutcome {
            snapshot_world_generation: snapshot_identity.world_generation,
            snapshot_tick: snapshot_identity.tick,
            snapshot_lock_wait_ms,
            snapshot_lock_hold_ms,
            snapshot_capture_ms,
            snapshot_blocked_fixed_ticks,
            snapshot_wire_bytes,
            serialize_ms,
            write_ms: write_start.elapsed().as_secs_f64() * 1000.0,
            total_ms: worker_start.elapsed().as_secs_f64() * 1000.0,
            bytes: bytes.len(),
        })
    });
    if capture_started_rx.recv().is_err() {
        let error = join_job(handle)
            .err()
            .unwrap_or_else(|| "save worker stopped before snapshot capture began".to_string());
        status.message = Some(format!("Cannot save {display_name}: {error}"));
        status.kind = SaveLoadStatusKind::Error;
        return false;
    }
    pending.jobs.push(SaveJob {
        id,
        display_name: display_name.clone(),
        normalized_name,
        explicit,
        handle,
    });
    metrics.last_request_submission_ms = submission_start.elapsed().as_secs_f64() * 1000.0;
    if explicit {
        status.message = Some(format!("Saving {display_name}..."));
        status.kind = SaveLoadStatusKind::Info;
        status.last_completed_id = None;
    }
    true
}

pub(crate) fn take_completed(pending: &mut PendingSaveJobs) -> Vec<CompletedJob> {
    let mut completed = Vec::new();
    let mut index = 0;
    while index < pending.jobs.len() {
        if !pending.jobs[index].handle.is_finished() {
            index += 1;
        } else {
            let job = pending.jobs.swap_remove(index);
            let result = join_job(job.handle);
            completed.push(CompletedJob {
                id: job.id,
                display_name: job.display_name,
                explicit: job.explicit,
                result,
            });
        }
    }
    completed
}

fn join_job(handle: JoinHandle<Result<SaveJobOutcome, String>>) -> Result<SaveJobOutcome, String> {
    handle
        .join()
        .unwrap_or_else(|_| Err("save worker panicked".into()))
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

    #[test]
    fn only_an_active_worker_consumes_generation_capacity() {
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let handle = thread::spawn(move || {
            release_rx.recv().unwrap();
            Err("intentional test result".into())
        });
        let mut pending = PendingSaveJobs {
            jobs: vec![SaveJob {
                id: SaveId::new("test"),
                display_name: "Test".into(),
                normalized_name: Some("test".into()),
                explicit: true,
                handle,
            }],
        };

        assert!(pending.any_running());
        assert!(pending.is_at_capacity());
        assert!(pending.is_id_pending(&SaveId::new("test")));
        assert!(pending.is_name_pending("test"));
        assert_eq!(pending.pending_ids(), [SaveId::new("test")]);

        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pending.jobs[0].handle.is_finished() {
            assert!(Instant::now() < deadline, "test worker did not finish");
            thread::yield_now();
        }

        assert!(!pending.any_running());
        assert!(!pending.is_at_capacity());
        assert!(pending.is_id_pending(&SaveId::new("test")));
        assert!(pending.is_name_pending("test"));
        assert_eq!(pending.pending_ids(), [SaveId::new("test")]);
        assert_eq!(take_completed(&mut pending).len(), 1);
        assert!(pending.is_empty());
    }
}
