use super::catalog::now_unix_ms;
use super::container::{METADATA_SCHEMA_VERSION, encode_container, write_save_bytes};
use super::{
    SaveId, SaveKind, SaveLoadConfig, SaveLoadMetrics, SaveLoadStatus, SaveLoadStatusKind,
    SaveMetadata,
};
use crate::resources::SimResource;
use factory_sim::save_to_bytes;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[derive(bevy::prelude::Resource, Default)]
pub struct PendingSaveJobs {
    jobs: Vec<SaveJob>,
}

impl PendingSaveJobs {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
    pub fn any_running(&self) -> bool {
        !self.jobs.is_empty()
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
}

struct SaveJob {
    id: SaveId,
    display_name: String,
    normalized_name: Option<String>,
    explicit: bool,
    pollable: bool,
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
    pub lock_wait_ms: f64,
    pub serialize_ms: f64,
    pub write_ms: f64,
    pub total_ms: f64,
    pub bytes: usize,
}

#[allow(clippy::too_many_arguments)]
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
    let submission_start = Instant::now();
    let sim = sim.clone_handle();
    let worker_id = id.clone();
    let worker_name = display_name.clone();
    let handle = thread::spawn(move || {
        let worker_start = Instant::now();
        let lock_start = Instant::now();
        let sim = sim
            .read()
            .map_err(|_| "simulation lock poisoned".to_string())?;
        let lock_wait_ms = lock_start.elapsed().as_secs_f64() * 1000.0;
        let serialize_start = Instant::now();
        let payload = save_to_bytes(&sim)
            .map_err(|error| format!("simulation serialization failed: {error:?}"))?;
        let serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;
        drop(sim);
        let metadata = SaveMetadata {
            schema_version: METADATA_SCHEMA_VERSION,
            id: worker_id,
            display_name: worker_name,
            kind,
            completed_at_unix_ms: now_unix_ms(),
            application_version: env!("CARGO_PKG_VERSION").into(),
        };
        let bytes = encode_container(&metadata, &payload).map_err(|error| error.to_string())?;
        let write_start = Instant::now();
        write_save_bytes(&path, &bytes).map_err(|error| error.to_string())?;
        Ok(SaveJobOutcome {
            lock_wait_ms,
            serialize_ms,
            write_ms: write_start.elapsed().as_secs_f64() * 1000.0,
            total_ms: worker_start.elapsed().as_secs_f64() * 1000.0,
            bytes: bytes.len(),
        })
    });
    pending.jobs.push(SaveJob {
        id,
        display_name: display_name.clone(),
        normalized_name,
        explicit,
        pollable: false,
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
        if !pending.jobs[index].pollable {
            pending.jobs[index].pollable = true;
            index += 1;
        } else if !pending.jobs[index].handle.is_finished() {
            index += 1;
        } else {
            let job = pending.jobs.swap_remove(index);
            let result = job
                .handle
                .join()
                .unwrap_or_else(|_| Err("save worker panicked".into()));
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

pub(crate) fn system_path(config: &SaveLoadConfig, kind: &SaveKind) -> PathBuf {
    match kind {
        SaveKind::Named => unreachable!("named saves use generated paths"),
        SaveKind::Quicksave => config.root_dir.join("quicksave.factsim"),
        SaveKind::Autosave { generation } => config
            .root_dir
            .join(format!("autosave-{generation}.factsim")),
    }
}
