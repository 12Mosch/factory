use super::commit::{CommitFaultPhase, CommitFaults, SaveDurability};
use super::lifecycle::{SAVE_CANCEL_ACTIVE, SAVE_CANCEL_COMMITTING};
use super::{SaveId, SaveKind, SaveMetadata};
use factory_sim::{
    RECORD_MAGIC, SAVE_HEADER_SIZE, SaveLimits, SaveLoadError, Simulation, SimulationSaveSnapshot,
    load_from_reader_with_limits, save_snapshot_records_to_writer_with_limits,
};
use std::collections::BTreeMap;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, TryLockError};
use std::time::{Duration, Instant};
use std::{fs, str};

static SAVE_ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(0);
static SAVE_ARTIFACT_LOCK: Mutex<()> = Mutex::new(());
/// Per-path mutation epochs for canonical save artifacts, drawn from a
/// process-wide commit sequence. Bumped after every committed replacement
/// or removal (save commits, recovery promotions, deletions); deleted
/// paths are reclaimed. Catalog validation outcomes and load candidates
/// record the epoch alongside worker observations so the frame can reject
/// results overtaken by our own writer without any filesystem call.
/// Per-path (not global) so a save to one slot never invalidates another
/// path's in-flight work. The sequence never reuses a value, so deleting
/// (reclaim) then recreating a path cannot realign with an older recorded
/// epoch — no ABA.
static SAVE_ARTIFACT_EPOCHS: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());
static SAVE_COMMIT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn save_artifact_epoch(path: &Path) -> u64 {
    SAVE_ARTIFACT_EPOCHS
        .lock()
        .map(|epochs| epochs.get(path).copied().unwrap_or(0))
        .unwrap_or(0)
}

pub(crate) fn bump_save_artifact_epoch(path: &Path) {
    if let Ok(mut epochs) = SAVE_ARTIFACT_EPOCHS.lock() {
        epochs.insert(
            path.to_path_buf(),
            SAVE_COMMIT_SEQUENCE.fetch_add(1, Ordering::AcqRel),
        );
    }
}

pub(crate) fn reclaim_save_artifact_epoch(path: &Path) {
    if let Ok(mut epochs) = SAVE_ARTIFACT_EPOCHS.lock() {
        epochs.remove(path);
    }
}

/// Magic bytes at the start of a Factory save container.
pub const CONTAINER_MAGIC: [u8; 8] = *b"FACTSAVE";
/// Current Factory save-container format version.
pub const CONTAINER_VERSION: u32 = 1;
/// v2 records the world seed in save metadata so catalogs can display it
/// without deserializing the simulation payload. v1 metadata decodes with
/// `world_seed: None`.
pub const METADATA_SCHEMA_VERSION: u32 = 2;
/// Maximum serialized metadata size accepted by the container parser.
pub const MAX_METADATA_BYTES: usize = factory_sim::save_limits::SAVE_METADATA_BYTES;
/// Marker separating a canonical save name from a temporary artifact nonce.
pub const TEMP_ARTIFACT_MARKER: &str = ".tmp-";
/// Marker separating a canonical save name from a backup artifact nonce.
pub const BACKUP_ARTIFACT_MARKER: &str = ".bak-";
const PREFIX_SIZE: usize = factory_sim::save_limits::SAVE_CONTAINER_PREFIX_BYTES;
const RETIRED_ARTIFACT_SUFFIX: &str = ".retired";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveArtifactKind {
    Temporary,
    Backup,
}

/// Error produced while encoding, inspecting, or decoding a save container.
#[derive(Debug)]
pub enum ContainerError {
    TooLarge,
    UnsupportedVersion(u32),
    Io(io::Error),
    MetadataTooLarge(usize),
    MetadataEncoding(String),
    Truncated,
    InvalidContainerMagic,
    Simulation(SaveLoadError),
    /// The request was cancelled before the commit point. Nothing was
    /// installed; temporary artifacts are removed by the writer.
    Cancelled,
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(formatter, "save exceeds this build's save size limits"),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported save container version {version} (supported: {CONTAINER_VERSION})"
            ),
            Self::Io(error) => write!(formatter, "save I/O failed: {error}"),
            Self::MetadataTooLarge(size) => write!(
                formatter,
                "metadata is {size} bytes (maximum is {MAX_METADATA_BYTES})"
            ),
            Self::MetadataEncoding(error) => write!(formatter, "metadata encoding failed: {error}"),
            Self::Truncated => write!(formatter, "save container is truncated"),
            Self::InvalidContainerMagic => write!(formatter, "invalid save container magic"),
            Self::Simulation(error) => write!(formatter, "simulation codec failed: {error:?}"),
            Self::Cancelled => write!(formatter, "save cancelled before commit"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StreamWriteMetrics {
    pub total_bytes: usize,
    pub simulation_bytes: usize,
    pub encode_ms: f64,
    /// Post-commit durability barrier outcome. A degraded barrier is still a
    /// commit (see [`SaveDurability`]); it must surface in status instead of
    /// reading as fully durable or failing as I/O.
    pub durability: SaveDurability,
}

impl From<io::Error> for ContainerError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub(crate) struct InspectedContainer {
    pub version: u32,
    pub metadata: Option<SaveMetadata>,
    pub simulation_header: Vec<u8>,
}

/// Encodes metadata and a simulation payload into the current container format.
pub fn encode_container(
    metadata: &SaveMetadata,
    payload: &[u8],
) -> Result<Vec<u8>, ContainerError> {
    encode_container_with_limits(metadata, payload, SaveLimits::default())
}

fn encode_container_with_limits(
    metadata: &SaveMetadata,
    payload: &[u8],
    limits: SaveLimits,
) -> Result<Vec<u8>, ContainerError> {
    let metadata_text = ron::ser::to_string(metadata)
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    let metadata_bytes = metadata_text.as_bytes();
    if metadata_bytes.len() > limits.max_metadata_bytes {
        return Err(ContainerError::MetadataTooLarge(metadata_bytes.len()));
    }
    let metadata_len = u32::try_from(metadata_bytes.len())
        .map_err(|_| ContainerError::MetadataTooLarge(metadata_bytes.len()))?;
    let payload_offset = (PREFIX_SIZE + metadata_bytes.len()) as u64;
    check_size(
        payload.len() as u64,
        simulation_payload_allowance(payload, payload_offset, limits),
    )?;
    check_size(
        payload_offset + payload.len() as u64,
        limits.max_encoded_bytes,
    )?;
    let mut bytes = Vec::with_capacity(PREFIX_SIZE + metadata_bytes.len() + payload.len());
    bytes.extend_from_slice(&CONTAINER_MAGIC);
    bytes.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    bytes.extend_from_slice(&metadata_len.to_le_bytes());
    bytes.extend_from_slice(metadata_bytes);
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

/// Payload allowance for an inner simulation payload: record containers
/// carry header and manifest framing inside the payload, so they are bounded
/// by the artifact budget, while monolithic payloads keep the legacy
/// allowance. Unknown inner magics stay conservative.
fn simulation_payload_allowance(
    inner_magic: &[u8],
    payload_offset: u64,
    limits: SaveLimits,
) -> u64 {
    if inner_magic.starts_with(&RECORD_MAGIC) {
        limits.max_encoded_bytes.saturating_sub(payload_offset)
    } else {
        limits
            .max_simulation_bytes()
            .min(limits.max_encoded_bytes.saturating_sub(payload_offset))
    }
}

/// Decodes container metadata and returns a borrowed simulation payload.
pub fn decode_container(bytes: &[u8]) -> Result<(SaveMetadata, &[u8]), ContainerError> {
    let payload_offset = container_payload_offset(bytes)?;
    check_size(bytes.len() as u64, SaveLimits::default().max_encoded_bytes)?;
    check_size(
        (bytes.len() - payload_offset) as u64,
        simulation_payload_allowance(
            bytes.get(payload_offset..).unwrap_or(&[]),
            payload_offset as u64,
            SaveLimits::default(),
        ),
    )?;
    let metadata = ron::de::from_bytes(&bytes[PREFIX_SIZE..payload_offset])
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    Ok((metadata, &bytes[payload_offset..]))
}

/// Validates the fixed prefix and computes the first payload byte.
pub(crate) fn container_payload_offset(bytes: &[u8]) -> Result<usize, ContainerError> {
    container_payload_offset_with_limits(bytes, SaveLimits::default())
}

fn container_payload_offset_with_limits(
    bytes: &[u8],
    limits: SaveLimits,
) -> Result<usize, ContainerError> {
    if bytes.len() < PREFIX_SIZE {
        return Err(ContainerError::Truncated);
    }
    if bytes[..8] != CONTAINER_MAGIC {
        return Err(ContainerError::InvalidContainerMagic);
    }
    check_version(u32::from_le_bytes(
        bytes[8..12].try_into().expect("fixed range"),
    ))?;
    let metadata_len = u32::from_le_bytes(bytes[12..16].try_into().expect("fixed range")) as usize;
    if metadata_len > limits.max_metadata_bytes {
        return Err(ContainerError::MetadataTooLarge(metadata_len));
    }
    let payload_offset = PREFIX_SIZE
        .checked_add(metadata_len)
        .ok_or(ContainerError::Truncated)?;
    if bytes.len() < payload_offset {
        return Err(ContainerError::Truncated);
    }
    Ok(payload_offset)
}

/// Reads only the container metadata and simulation header needed by the catalog.
pub(crate) fn inspect_container(path: &Path) -> Result<InspectedContainer, ContainerError> {
    inspect_container_from_reader(&mut fs::File::open(path)?)
}

/// Reads container metadata and simulation header from an already-open handle
/// so callers can bind the classification to that file instance.
pub(crate) fn inspect_container_from_reader(
    reader: &mut impl Read,
) -> Result<InspectedContainer, ContainerError> {
    let mut prefix = [0; PREFIX_SIZE];
    read_inspection_bytes(&mut *reader, &mut prefix)?;
    if prefix[..8] != CONTAINER_MAGIC {
        return Err(ContainerError::InvalidContainerMagic);
    }
    let version = u32::from_le_bytes(prefix[8..12].try_into().expect("fixed range"));
    let metadata_len = u32::from_le_bytes(prefix[12..16].try_into().expect("fixed range")) as usize;
    if metadata_len > MAX_METADATA_BYTES {
        return Err(ContainerError::MetadataTooLarge(metadata_len));
    }
    let mut metadata_bytes = vec![0; metadata_len];
    read_inspection_bytes(&mut *reader, &mut metadata_bytes)?;
    let metadata = ron::de::from_bytes(&metadata_bytes).ok();
    let mut simulation_header = vec![0; SAVE_HEADER_SIZE];
    read_inspection_bytes(&mut *reader, &mut simulation_header)?;
    Ok(InspectedContainer {
        version,
        metadata,
        simulation_header,
    })
}

/// Treats a short file as corruption while retaining every other read failure
/// as an I/O error so recovery cannot replace a primary it could not inspect.
pub(crate) fn read_inspection_bytes(
    reader: &mut impl Read,
    buffer: &mut [u8],
) -> Result<(), ContainerError> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            ContainerError::Truncated
        } else {
            ContainerError::Io(error)
        }
    })
}

/// Streams a container (or legacy raw quicksave) into a detached candidate
/// simulation. The caller remains responsible for installing that candidate.
pub(crate) fn load_simulation(path: &Path) -> Result<Simulation, ContainerError> {
    let file = fs::File::open(path)?;
    load_simulation_from_reader(&mut BufReader::new(file), SaveLimits::default())
}

pub(crate) fn load_simulation_from_reader(
    reader: &mut impl Read,
    limits: SaveLimits,
) -> Result<Simulation, ContainerError> {
    check_size(SAVE_HEADER_SIZE as u64, limits.max_simulation_bytes())?;
    let mut magic = [0; 8];
    read_inspection_bytes(reader, &mut magic)?;
    if magic != CONTAINER_MAGIC {
        let mut raw = io::Cursor::new(magic).chain(reader);
        return load_from_reader_with_limits(&mut raw, limits).map_err(map_simulation_error);
    }

    let mut prefix = [0; 8];
    read_inspection_bytes(reader, &mut prefix)?;
    check_version(u32::from_le_bytes(
        prefix[..4].try_into().expect("fixed range"),
    ))?;
    let metadata_len = u32::from_le_bytes(prefix[4..].try_into().expect("fixed range")) as usize;
    if metadata_len > limits.max_metadata_bytes {
        return Err(ContainerError::MetadataTooLarge(metadata_len));
    }
    let overhead = PREFIX_SIZE as u64 + metadata_len as u64;
    check_size(overhead, limits.max_encoded_bytes)?;
    let copied = io::copy(&mut reader.take(metadata_len as u64), &mut io::sink())?;
    if copied != metadata_len as u64 {
        return Err(ContainerError::Truncated);
    }
    // Peek the inner magic before choosing the payload allowance: record
    // payloads may use framing up to the artifact budget.
    let mut inner_magic = [0; 8];
    read_inspection_bytes(reader, &mut inner_magic)?;
    let maximum = simulation_payload_allowance(&inner_magic, overhead, limits);
    let mut payload = io::Cursor::new(inner_magic).chain(reader.take(maximum.saturating_add(1)));
    let simulation_limits = SaveLimits {
        max_encoded_bytes: maximum,
        ..limits
    };
    load_from_reader_with_limits(&mut payload, simulation_limits).map_err(map_simulation_error)
}

/// Keeps external reader failures distinct from malformed simulation bytes so
/// recovery never replaces a primary that merely became temporarily unreadable.
fn map_simulation_error(error: SaveLoadError) -> ContainerError {
    match error {
        SaveLoadError::TooLarge => ContainerError::TooLarge,
        error => match error.into_io_error() {
            Ok(error) => ContainerError::Io(error),
            Err(error) => ContainerError::Simulation(error),
        },
    }
}

/// Recovery retains artifact bytes for exact duplicate comparisons, but never
/// copies their payload into a second allocation.
pub(crate) fn read_save_artifact(path: &Path) -> Result<Vec<u8>, ContainerError> {
    read_bounded_bytes(
        &mut fs::File::open(path)?,
        Vec::new(),
        SaveLimits::default().max_encoded_bytes,
    )
}

fn check_size(size: u64, maximum: u64) -> Result<(), ContainerError> {
    if size > maximum {
        Err(ContainerError::TooLarge)
    } else {
        Ok(())
    }
}

fn check_version(version: u32) -> Result<(), ContainerError> {
    if version != CONTAINER_VERSION {
        Err(ContainerError::UnsupportedVersion(version))
    } else {
        Ok(())
    }
}

fn read_bounded_bytes(
    reader: &mut impl Read,
    mut payload: Vec<u8>,
    maximum: u64,
) -> Result<Vec<u8>, ContainerError> {
    check_size(payload.len() as u64, maximum)?;
    // Cap both reads and capacity growth, allowing one excess byte for detection.
    let mut buffer = [0; 8192];
    loop {
        let remaining = maximum - payload.len() as u64;
        let count = (buffer.len() as u64).min(remaining.saturating_add(1)) as usize;
        let read = match reader.read(&mut buffer[..count]) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        check_size(read as u64, remaining)?;
        if payload.len() + read > payload.capacity() {
            let capacity = (payload.capacity() as u64)
                .saturating_mul(2)
                .max((payload.len() + read) as u64)
                .min(maximum) as usize;
            payload.reserve_exact(capacity - payload.len());
        }
        payload.extend_from_slice(&buffer[..read]);
    }
    Ok(payload)
}

/// Test-only hook that holds the save-artifact lock across frames, letting
/// responsiveness tests prove catalog refreshes never block on the writer.
#[doc(hidden)]
pub fn hold_save_artifact_lock_for_tests() -> std::sync::MutexGuard<'static, ()> {
    SAVE_ARTIFACT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Serializes save-directory mutations across the catalog and background writer.
pub(crate) fn with_save_artifact_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = SAVE_ARTIFACT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operation()
}

/// Non-blocking acquisition for frame-side installation: returns `None` when
/// the artifact lock is held (e.g. a save worker encoding or a scan worker
/// in recovery) instead of stalling the frame. The caller retains its
/// candidate and retries next frame. While the guard is held, no save commit
/// can land, so the freshness check and the world installation under it are
/// atomic with respect to our own writer.
pub(crate) fn try_acquire_save_artifact_lock() -> Option<std::sync::MutexGuard<'static, ()>> {
    match SAVE_ARTIFACT_LOCK.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poison)) => Some(poison.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Polling interval while waiting for the artifact lock with shutdown
/// admission. Keeps teardown latency near-instant without busy-spinning
/// against a holder doing real work.
const ARTIFACT_LOCK_WAIT_POLL: Duration = Duration::from_millis(1);

/// Like [`with_save_artifact_lock`], but stops waiting when `shutdown` is
/// set (resource teardown racing a detached lock holder, e.g. a catalog
/// scan worker detached at shutdown while holding the lock across
/// recovery) instead of blocking indefinitely. Returns `None` when
/// shutdown won the race; once the lock is held the operation still runs
/// to completion.
pub(crate) fn with_save_artifact_lock_shutdown_aware<T>(
    shutdown: &AtomicBool,
    operation: impl FnOnce() -> T,
) -> Option<T> {
    let guard = loop {
        match SAVE_ARTIFACT_LOCK.try_lock() {
            Ok(guard) => break guard,
            Err(TryLockError::Poisoned(poison)) => break poison.into_inner(),
            Err(TryLockError::WouldBlock) => {
                if shutdown.load(Ordering::Relaxed) {
                    return None;
                }
                std::thread::sleep(ARTIFACT_LOCK_WAIT_POLL);
            }
        }
    };
    let result = operation();
    drop(guard);
    Some(result)
}

/// Writes and installs a complete save without exposing partial contents.
///
/// Returns the durability barrier outcome: a degraded barrier is still a
/// commit (see [`SaveDurability`]), never an error that would invite an
/// unsafe overwrite retry.
pub(crate) fn write_save_bytes(
    path: &Path,
    bytes: &[u8],
) -> Result<SaveDurability, ContainerError> {
    write_save_bytes_with_faults(path, bytes, &CommitFaults::none())
}

/// Fault-injecting variant of [`write_save_bytes`] for deterministic tests.
/// Each [`CommitFaultPhase`] failure proves the recovery invariant at one
/// interrupted boundary (disk-full, permission, locked, partial I/O).
pub(crate) fn write_save_bytes_with_faults(
    path: &Path,
    bytes: &[u8],
    faults: &CommitFaults,
) -> Result<SaveDurability, ContainerError> {
    with_save_artifact_lock(|| write_save_bytes_locked(path, bytes, SaveLimits::default(), faults))
}

/// Encodes a snapshot through a buffered temporary file and commits it only
/// after the encoder has finished and the buffer has been flushed and synced.
/// Admits `shutdown` while waiting for the artifact lock — which a detached
/// scan worker may hold across recovery — instead of joining shutdown
/// through it. Returns `None` when shutdown was signalled during the wait;
/// the caller reports cancellation. Abandoning the wait never loses a
/// save: nothing has committed yet, and leftover temp artifacts are
/// recovered next startup.
pub(crate) fn write_save_snapshot(
    path: &Path,
    metadata: &SaveMetadata,
    snapshot: &SimulationSaveSnapshot,
    shutdown: &AtomicBool,
    cancel: &AtomicU8,
) -> Option<Result<StreamWriteMetrics, ContainerError>> {
    write_save_snapshot_with_faults(
        path,
        metadata,
        snapshot,
        shutdown,
        cancel,
        &CommitFaults::none(),
    )
}

/// Fault-injecting variant of [`write_save_snapshot`] for deterministic tests.
pub(crate) fn write_save_snapshot_with_faults(
    path: &Path,
    metadata: &SaveMetadata,
    snapshot: &SimulationSaveSnapshot,
    shutdown: &AtomicBool,
    cancel: &AtomicU8,
    faults: &CommitFaults,
) -> Option<Result<StreamWriteMetrics, ContainerError>> {
    with_save_artifact_lock_shutdown_aware(shutdown, || {
        write_save_snapshot_locked(
            path,
            metadata,
            snapshot,
            SaveLimits::default(),
            cancel,
            faults,
        )
    })
}

fn write_save_snapshot_locked(
    path: &Path,
    metadata: &SaveMetadata,
    snapshot: &SimulationSaveSnapshot,
    limits: SaveLimits,
    cancel: &AtomicU8,
    faults: &CommitFaults,
) -> Result<StreamWriteMetrics, ContainerError> {
    let metadata_text = ron::ser::to_string(metadata)
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    let metadata_bytes = metadata_text.as_bytes();
    if metadata_bytes.len() > limits.max_metadata_bytes {
        return Err(ContainerError::MetadataTooLarge(metadata_bytes.len()));
    }
    let metadata_len = u32::try_from(metadata_bytes.len())
        .map_err(|_| ContainerError::MetadataTooLarge(metadata_bytes.len()))?;
    let overhead = PREFIX_SIZE as u64 + metadata_bytes.len() as u64;
    check_size(overhead, limits.max_encoded_bytes)?;
    // The record header and manifest are framing inside the payload, so the
    // payload allowance is the artifact budget minus the outer container
    // overhead — not the legacy monolithic allowance.
    let payload_maximum = limits.max_encoded_bytes - overhead;
    // Scope the writer to the payload allowance so its pre-write total check
    // guarantees the records fit before any byte reaches the file.
    let record_limits = SaveLimits {
        max_encoded_bytes: payload_maximum,
        ..limits
    };

    let (partial, durability) = write_temporary_and_commit(
        path,
        |writer| {
            // Partial-I/O simulation: when the Write fault is set, leave a
            // flushed partial prefix on disk (not just buffered bytes) so
            // cleanup must remove a real partial file while the previous
            // save stays intact. Disk-full and permission failures share
            // this pre-commit contract; locked files surface as
            // `PermissionDenied`, matching Windows sharing violations.
            if faults.fails_at(CommitFaultPhase::Write) {
                writer.write_all(&CONTAINER_MAGIC)?;
                writer.flush()?;
                return Err(ContainerError::Io(
                    faults
                        .check(CommitFaultPhase::Write)
                        .expect_err("Write fault must fire"),
                ));
            }
            writer.write_all(&CONTAINER_MAGIC)?;
            writer.write_all(&CONTAINER_VERSION.to_le_bytes())?;
            writer.write_all(&metadata_len.to_le_bytes())?;
            writer.write_all(metadata_bytes)?;
            let encode_start = Instant::now();
            let simulation_bytes = {
                let mut payload = LimitedWriter::new(writer, payload_maximum);
                save_snapshot_records_to_writer_with_limits(snapshot, &mut payload, record_limits)
                    .map_err(map_simulation_error)?;
                payload.written
            };
            Ok((
                overhead as usize + simulation_bytes,
                simulation_bytes,
                encode_start.elapsed().as_secs_f64() * 1000.0,
            ))
        },
        cancel,
        faults,
    )?;
    let (total_bytes, simulation_bytes, encode_ms) = partial;
    Ok(StreamWriteMetrics {
        total_bytes,
        simulation_bytes,
        encode_ms,
        durability,
    })
}

/// Implements save installation while the process-wide artifact lock is held.
fn write_save_bytes_locked(
    path: &Path,
    bytes: &[u8],
    limits: SaveLimits,
    faults: &CommitFaults,
) -> Result<SaveDurability, ContainerError> {
    // The synchronous UI path is never cancelled; the worker path threads
    // its request flag through `write_save_snapshot_locked` instead.
    let cancel = AtomicU8::new(SAVE_CANCEL_ACTIVE);
    let ((), durability) = write_temporary_and_commit(
        path,
        |temp| {
            check_size(bytes.len() as u64, limits.max_encoded_bytes)?;
            if bytes.starts_with(&CONTAINER_MAGIC) {
                let offset = container_payload_offset_with_limits(bytes, limits)?;
                check_size(
                    (bytes.len() - offset) as u64,
                    simulation_payload_allowance(
                        bytes.get(offset..).unwrap_or(&[]),
                        offset as u64,
                        limits,
                    ),
                )?;
            } else {
                check_size(bytes.len() as u64, limits.max_simulation_bytes())?;
            }
            // Partial-I/O simulation: flush a real partial prefix before
            // failing so cleanup removes on-disk partial bytes, not just an
            // empty buffered file.
            if faults.fails_at(CommitFaultPhase::Write) {
                let partial = bytes.len().saturating_add(1) / 2;
                temp.write_all(&bytes[..partial])?;
                temp.flush()?;
                return Err(ContainerError::Io(
                    faults
                        .check(CommitFaultPhase::Write)
                        .expect_err("Write fault must fire"),
                ));
            }
            temp.write_all(bytes)?;
            Ok(())
        },
        &cancel,
        faults,
    )?;
    Ok(durability)
}

/// Upper bound for the ancestor walk below: far beyond any real save-root
/// nesting, while keeping a pathological path from stalling the commit.
const MAX_ANCESTOR_WALK: usize = 256;

/// Splits the save directory into newly missing components (child-first)
/// plus the deepest pre-existing ancestor that links the new chain.
///
/// The post-install barrier syncs every created directory and that linking
/// parent: without those barriers a crash can remove the whole new tree
/// even though the save itself reported durable.
fn missing_ancestor_chain(path: &Path) -> (Vec<PathBuf>, Option<PathBuf>) {
    let mut missing = Vec::new();
    let mut cursor = path.parent().map(Path::to_path_buf);
    for _ in 0..MAX_ANCESTOR_WALK {
        let Some(dir) = cursor else { break };
        if dir.as_os_str().is_empty() {
            break;
        }
        match dir.try_exists() {
            Ok(true) => return (missing, Some(dir)),
            // Unstatable ancestors fail in `create_dir_all` below when they
            // truly block creation; syncing them post-install is harmless
            // when they merely raced into existence.
            _ => {
                missing.push(dir.clone());
                cursor = dir.parent().map(Path::to_path_buf);
            }
        }
    }
    (missing, None)
}

/// Syncs every created directory plus the pre-existing linking parent so a
/// first save in a new root is as durable as it reports. Any failure
/// degrades the commit instead of reading as fully durable.
fn sync_created_ancestors(missing: &[PathBuf], base: Option<&Path>) -> io::Result<()> {
    for dir in missing {
        sync_directory(dir)?;
    }
    if let Some(base) = base {
        sync_directory(base)?;
    }
    Ok(())
}

fn write_temporary_and_commit<T>(
    path: &Path,
    encode: impl FnOnce(&mut BufWriter<fs::File>) -> Result<T, ContainerError>,
    cancel: &AtomicU8,
    faults: &CommitFaults,
) -> Result<(T, SaveDurability), ContainerError> {
    faults
        .check(CommitFaultPhase::CreateDir)
        .map_err(ContainerError::Io)?;
    // Record the ancestor chain before creating anything: the post-install
    // barrier syncs every created directory plus the pre-existing parent
    // that links the new chain, so a first save in a new root never reports
    // durable while its own directories are still crash-removable.
    let (missing_ancestors, ancestor_base) = missing_ancestor_chain(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let counter = SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let nonce = format!("{}-{timestamp:x}-{counter:x}", std::process::id());
    let temp_path = save_artifact_path(path, SaveArtifactKind::Temporary, &nonce);
    let backup_path = save_artifact_path(path, SaveArtifactKind::Backup, &nonce);
    let mut installed = false;

    let result = (|| {
        faults
            .check(CommitFaultPhase::CreateTemp)
            .map_err(ContainerError::Io)?;
        let temp = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        let mut temp = BufWriter::new(temp);
        let outcome = encode(&mut temp)?;
        // Fault after the encoder ran but before the flush: the temporary
        // file holds partial contents that cleanup must remove while the
        // previous save stays intact.
        faults
            .check(CommitFaultPhase::Write)
            .map_err(ContainerError::Io)?;
        faults
            .check(CommitFaultPhase::Flush)
            .map_err(ContainerError::Io)?;
        temp.flush()?;
        faults
            .check(CommitFaultPhase::SyncTemp)
            .map_err(ContainerError::Io)?;
        temp.get_ref().sync_all()?;
        drop(temp);
        faults
            .check(CommitFaultPhase::SyncParentPre)
            .map_err(ContainerError::Io)?;
        sync_parent_directory(path)?;

        // The atomic commit point: the worker claims ACTIVE -> COMMITTING
        // with a single compare-exchange before the rename makes the
        // replacement visible. A cancel racing the encode wins
        // (ACTIVE -> REQUESTED) and aborts here; once the worker claims
        // committing, a later cancel observes COMMITTING and reports too
        // late instead of a false success. The error path below removes
        // the temporary artifact, so a cancelled request leaves the
        // previous save intact. Once the rename begins the operation
        // counts as committed.
        if cancel
            .compare_exchange(
                SAVE_CANCEL_ACTIVE,
                SAVE_CANCEL_COMMITTING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return Err(ContainerError::Cancelled);
        }
        let replaced = commit_temporary_file(path, &temp_path, &backup_path, faults)?;
        installed = true;
        // Installation has committed. A durability-barrier failure degrades
        // durability instead of failing: retrying could overwrite success.
        // See `SaveDurability` and `commit::format_save_success`.
        let durability = match faults.sync_barrier_error() {
            Some(error) => SaveDurability::unsynced(error),
            None => {
                match sync_installed_file(path).and_then(|()| {
                    sync_created_ancestors(&missing_ancestors, ancestor_base.as_deref())
                }) {
                    Ok(()) => SaveDurability::Durable,
                    Err(error) => SaveDurability::unsynced(error),
                }
            }
        };

        if replaced {
            // The new primary is committed. Cleanup cannot turn that successful
            // save into an error; catalog refresh retries any leftover backup.
            // When the barrier already failed, cleanup removes files without
            // issuing further syncs: a later successful parent sync would
            // otherwise make the rename durable after this commit was already
            // classified as unsynced, so the degraded verdict must match a
            // state with no successful barrier after the rename.
            if durability.degraded_reason().is_some() {
                let _ = fs::remove_file(&backup_path);
            } else {
                let _ = discard_save_artifact_with_faults(&backup_path, faults);
                if faults.check(CommitFaultPhase::SyncParentPost).is_ok() {
                    let _ = sync_parent_directory(path);
                }
            }
        }
        Ok((outcome, durability))
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
        if !installed {
            let _ = fs::remove_file(&backup_path);
        }
        let _ = sync_parent_directory(path);
    } else {
        bump_save_artifact_epoch(path);
    }
    result
}

struct LimitedWriter<W> {
    inner: W,
    remaining: u64,
    written: usize,
}

impl<W> LimitedWriter<W> {
    fn new(inner: W, maximum: u64) -> Self {
        Self {
            inner,
            remaining: maximum,
            written: 0,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return Err(io::Error::other(
                "simulation payload exceeds container limit",
            ));
        }
        let allowed = usize::try_from(self.remaining.min(bytes.len() as u64))
            .expect("allowed write length is bounded by the source slice");
        let written = self.inner.write(&bytes[..allowed])?;
        self.remaining -= written as u64;
        self.written += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Atomically installs a validated backup while preserving a concurrently
/// created primary when recovery originally observed the path as missing.
pub(crate) fn promote_backup(
    backup_path: &Path,
    path: &Path,
    replace_corrupt_primary: bool,
) -> io::Result<()> {
    if replace_corrupt_primary {
        match replace_with_existing_file(path, backup_path) {
            Ok(()) => {}
            Err(_) if !path.try_exists()? => {
                install_new_file(backup_path, path, &CommitFaults::none())?
            }
            Err(error) => return Err(error),
        }
    } else {
        install_new_file(backup_path, path, &CommitFaults::none())?;
    }
    // Promotion has committed even if a post-rename durability barrier is not
    // available on this filesystem or is temporarily blocked by another handle.
    let _ = sync_installed_file(path);
    bump_save_artifact_epoch(path);
    Ok(())
}

/// Removes a canonical save and all of its recovery artifacts as one serialized
/// operation so an intentional deletion cannot be mistaken for a crashed write.
pub(crate) fn remove_save_and_artifacts(path: &Path) -> io::Result<()> {
    let result = with_save_artifact_lock(|| {
        for artifact in save_artifacts_for(path)? {
            discard_save_artifact(&artifact)?;
        }
        sync_parent_directory(path)?;
        fs::remove_file(path)?;
        sync_parent_directory(path)
    });
    if result.is_ok() {
        reclaim_save_artifact_epoch(path);
    }
    result
}

/// Durably removes an artifact, first retiring a backup so cleanup failure can
/// never leave an old snapshot eligible for automatic recovery.
pub(crate) fn discard_save_artifact(path: &Path) -> io::Result<()> {
    discard_save_artifact_with_faults(path, &CommitFaults::none())
}

fn discard_save_artifact_with_faults(path: &Path, faults: &CommitFaults) -> io::Result<()> {
    if parse_save_artifact(path).is_some_and(|(_, kind)| kind == SaveArtifactKind::Backup) {
        retire_recovery_artifact(path, faults)
    } else if path.try_exists()? {
        fs::remove_file(path)?;
        // A post-cleanup sync fault models sync failure after the removal:
        // files are still removed, only the barrier is skipped.
        if faults.fails_at(CommitFaultPhase::SyncParentPost) {
            return Ok(());
        }
        sync_parent_directory(path)
    } else {
        Ok(())
    }
}

/// Parses a temporary or backup artifact and returns its canonical save path.
pub(crate) fn parse_save_artifact(path: &Path) -> Option<(PathBuf, SaveArtifactKind)> {
    let file_name = path.file_name()?.to_str()?;
    for (marker, kind) in [
        (TEMP_ARTIFACT_MARKER, SaveArtifactKind::Temporary),
        (BACKUP_ARTIFACT_MARKER, SaveArtifactKind::Backup),
    ] {
        let Some((primary_name, nonce)) = file_name.rsplit_once(marker) else {
            continue;
        };
        if !primary_name.ends_with(".factsim")
            || nonce.is_empty()
            || !nonce
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return None;
        }
        return Some((path.with_file_name(primary_name), kind));
    }
    None
}

/// Returns the canonical path for a post-commit artifact that has already been
/// made ineligible for recovery.
pub(crate) fn retired_save_artifact_primary(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_str()?;
    let original_name = file_name.strip_suffix(RETIRED_ARTIFACT_SUFFIX)?;
    let original = path.with_file_name(original_name);
    parse_save_artifact(&original).map(|(primary, _)| primary)
}

/// Builds a sibling artifact path from the shared naming contract.
fn save_artifact_path(path: &Path, kind: SaveArtifactKind, nonce: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("save.factsim");
    let marker = match kind {
        SaveArtifactKind::Temporary => TEMP_ARTIFACT_MARKER,
        SaveArtifactKind::Backup => BACKUP_ARTIFACT_MARKER,
    };
    path.with_file_name(format!("{file_name}{marker}{nonce}"))
}

/// Finds active and retired artifacts belonging to one canonical save.
fn save_artifacts_for(path: &Path) -> io::Result<Vec<PathBuf>> {
    let Some(parent) = path.parent() else {
        return Ok(Vec::new());
    };
    let mut artifacts = Vec::new();
    for item in fs::read_dir(parent)? {
        let candidate = item?.path();
        let primary = parse_save_artifact(&candidate)
            .map(|(primary, _)| primary)
            .or_else(|| retired_save_artifact_primary(&candidate));
        if primary.as_deref() == Some(path) {
            artifacts.push(candidate);
        }
    }
    Ok(artifacts)
}

/// Atomically makes a committed backup ineligible before best-effort deletion.
fn retire_recovery_artifact(path: &Path, faults: &CommitFaults) -> io::Result<()> {
    faults.check(CommitFaultPhase::Retire)?;
    // A post-cleanup sync fault removes files without issuing further
    // barriers: the removal stays visible while durability stays best-effort.
    let skip_sync = faults.fails_at(CommitFaultPhase::SyncParentPost);
    if !path.try_exists()? {
        return Ok(());
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        fs::remove_file(path)?;
        if skip_sync {
            return Ok(());
        }
        return sync_parent_directory(path);
    };
    let retired = path.with_file_name(format!("{file_name}{RETIRED_ARTIFACT_SUFFIX}"));
    match rename_file_no_replace(path, &retired) {
        Ok(()) => {
            if !skip_sync {
                sync_parent_directory(path)?;
            }
            let _ = fs::remove_file(retired);
            if !skip_sync {
                let _ = sync_parent_directory(path);
            }
            Ok(())
        }
        Err(rename_error) => match fs::remove_file(path) {
            Ok(()) => {
                if skip_sync {
                    return Ok(());
                }
                sync_parent_directory(path)
            }
            Err(_) => Err(rename_error),
        },
    }
}

/// Installs a temporary file without a check-then-overwrite window.
fn commit_temporary_file(
    path: &Path,
    temp_path: &Path,
    backup_path: &Path,
    faults: &CommitFaults,
) -> io::Result<bool> {
    if path.try_exists()? {
        replace_file(path, temp_path, backup_path, faults)?;
        return Ok(true);
    }
    match install_new_file(temp_path, path, faults) {
        Ok(()) => Ok(false),
        Err(_) if path.try_exists()? => {
            replace_file(path, temp_path, backup_path, faults)?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

/// Creates a durable rollback link before atomically replacing the primary.
#[cfg(unix)]
fn replace_file(
    path: &Path,
    temp_path: &Path,
    backup_path: &Path,
    faults: &CommitFaults,
) -> io::Result<()> {
    faults.check(CommitFaultPhase::Backup)?;
    if fs::hard_link(path, backup_path).is_err() {
        fs::copy(path, backup_path)?;
        fs::File::open(backup_path)?.sync_all()?;
    }
    sync_parent_directory(path)?;
    faults.check(CommitFaultPhase::Rename)?;
    fs::rename(temp_path, path)
}

/// Atomically replaces the primary and asks Windows to retain its old contents.
#[cfg(windows)]
fn replace_file(
    path: &Path,
    temp_path: &Path,
    backup_path: &Path,
    faults: &CommitFaults,
) -> io::Result<()> {
    // ReplaceFileW is atomic: backup and replacement commit together, so a
    // fault at either phase prevents the call with the previous save intact.
    faults.check(CommitFaultPhase::Backup)?;
    faults.check(CommitFaultPhase::Rename)?;
    replace_file_windows(path, temp_path, Some(backup_path))
}

/// Portable fallback that copies the rollback snapshot before replacement.
#[cfg(not(any(unix, windows)))]
fn replace_file(
    path: &Path,
    temp_path: &Path,
    backup_path: &Path,
    faults: &CommitFaults,
) -> io::Result<()> {
    faults.check(CommitFaultPhase::Backup)?;
    fs::copy(path, backup_path)?;
    fs::File::open(backup_path)?.sync_all()?;
    sync_parent_directory(path)?;
    faults.check(CommitFaultPhase::Rename)?;
    fs::rename(temp_path, path)
}

/// Replaces a corrupt primary with an already validated backup on Windows.
#[cfg(windows)]
fn replace_with_existing_file(path: &Path, replacement_path: &Path) -> io::Result<()> {
    replace_file_windows(path, replacement_path, None)
}

/// Replaces a corrupt primary with an already validated backup via atomic rename.
#[cfg(not(windows))]
fn replace_with_existing_file(path: &Path, replacement_path: &Path) -> io::Result<()> {
    fs::rename(replacement_path, path)
}

/// Calls the native Windows replacement primitive with stable UTF-16 buffers.
#[cfg(windows)]
fn replace_file_windows(
    path: &Path,
    replacement_path: &Path,
    backup_path: Option<&Path>,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let replacement_path = replacement_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let backup_path = backup_path.map(|path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    });
    let backup_ptr = backup_path
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr());
    // SAFETY: all pointers reference NUL-terminated UTF-16 buffers that remain
    // alive for the duration of the call, and the reserved pointers are null.
    let replaced = unsafe {
        ReplaceFileW(
            path.as_ptr(),
            replacement_path.as_ptr(),
            backup_ptr,
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Installs a new Windows file without replacing a destination that appeared.
#[cfg(windows)]
fn install_new_file(temp_path: &Path, path: &Path, faults: &CommitFaults) -> io::Result<()> {
    faults.check(CommitFaultPhase::Rename)?;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let temp_path = temp_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers that stay
    // alive until the call returns.
    let moved = unsafe { MoveFileExW(temp_path.as_ptr(), path.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Renames an artifact on Windows without replacing an existing destination.
#[cfg(windows)]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    install_new_file(source, destination, &CommitFaults::none())
}

/// Uses Linux's atomic no-replace rename when hard links are unavailable.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let source = std::ffi::CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
    // SAFETY: both paths are NUL-terminated and valid for the duration of the
    // call; AT_FDCWD makes them relative to the process working directory.
    let renamed = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if renamed == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Uses Apple's atomic exclusive rename when hard links are unavailable.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let source = std::ffi::CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
    // SAFETY: both paths are NUL-terminated and valid for the duration of the
    // call; AT_FDCWD makes them relative to the process working directory.
    let renamed = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if renamed == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Moves an artifact through a no-clobber hard link where exclusive rename is unavailable.
#[cfg(all(
    not(windows),
    not(target_os = "linux"),
    not(target_os = "android"),
    not(target_os = "macos"),
    not(target_os = "ios")
))]
fn rename_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(source, destination)?;
    fs::remove_file(source)
}

/// Installs a new file through a hard link or an atomic no-replace rename.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
))]
fn install_new_file(temp_path: &Path, path: &Path, faults: &CommitFaults) -> io::Result<()> {
    faults.check(CommitFaultPhase::Rename)?;
    match fs::hard_link(temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(temp_path);
            Ok(())
        }
        Err(_) => rename_file_no_replace(temp_path, path),
    }
}

/// Installs through a no-clobber hard link where no exclusive rename API is available.
#[cfg(all(
    not(windows),
    not(target_os = "linux"),
    not(target_os = "android"),
    not(target_os = "macos"),
    not(target_os = "ios")
))]
fn install_new_file(temp_path: &Path, path: &Path, faults: &CommitFaults) -> io::Result<()> {
    faults.check(CommitFaultPhase::Rename)?;
    fs::hard_link(temp_path, path)?;
    let _ = fs::remove_file(temp_path);
    Ok(())
}

/// Flushes one directory's own metadata with a directory fsync on Unix.
#[cfg(unix)]
fn sync_directory(dir: &Path) -> io::Result<()> {
    fs::File::open(dir)?.sync_all()
}

/// Flushes one directory's own metadata through a backup-semantics handle.
#[cfg(windows)]
fn sync_directory(dir: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    fs::OpenOptions::new()
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)?
        .sync_all()
}

/// No-op where portable directory fsync is unavailable.
#[cfg(not(any(unix, windows)))]
fn sync_directory(_dir: &Path) -> io::Result<()> {
    Ok(())
}

/// Flushes containing-directory metadata with a directory fsync on Unix.
#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => sync_directory(parent),
        None => Ok(()),
    }
}

/// Flushes Windows directory metadata through a backup-semantics handle.
#[cfg(windows)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    sync_directory(parent)
}

/// No-op where portable directory fsync is unavailable.
#[cfg(not(any(unix, windows)))]
fn sync_parent_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Flushes the writable canonical file handle and its metadata on Windows.
#[cfg(windows)]
fn sync_installed_file(path: &Path) -> io::Result<()> {
    // std::fs cannot open a Windows directory without BACKUP_SEMANTICS, so use
    // the specialized parent-directory path after flushing the canonical file.
    let file_result = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all());
    let directory_result = sync_parent_directory(path);
    file_result.and(directory_result)
}

/// Uses directory fsync as the installation durability barrier on Unix.
#[cfg(unix)]
fn sync_installed_file(path: &Path) -> io::Result<()> {
    sync_parent_directory(path)
}

/// Reports degraded durability where no portable barrier exists: there is no
/// directory durability primitive to confirm the rename, so a commit must
/// never read as fully durable. Pre-commit directory sync stays a best-effort
/// no-op so saves still proceed; only the post-commit verdict degrades.
#[cfg(not(any(unix, windows)))]
fn sync_installed_file(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory durability barrier unavailable on this platform",
    ))
}

/// Builds catalog metadata when legacy or malformed metadata is unavailable.
pub(crate) fn fallback_metadata(
    id: SaveId,
    kind: SaveKind,
    display_name: String,
    timestamp: u64,
) -> SaveMetadata {
    SaveMetadata {
        schema_version: METADATA_SCHEMA_VERSION,
        id,
        display_name,
        kind,
        completed_at_unix_ms: timestamp,
        application_version: env!("CARGO_PKG_VERSION").into(),
        world_seed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{
        Simulation, inspect_record_index, load_from_bytes, save_snapshot_records_to_bytes,
        save_to_bytes, try_capture_save_snapshot,
    };

    struct FailingReader(io::ErrorKind);

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(self.0))
        }
    }

    struct FailAfterReader {
        bytes: io::Cursor<Vec<u8>>,
        fail_at: u64,
        kind: io::ErrorKind,
    }

    impl Read for FailAfterReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let position = self.bytes.position();
            if position >= self.fail_at {
                return Err(io::Error::from(self.kind));
            }
            let count = usize::try_from((self.fail_at - position).min(buffer.len() as u64))
                .expect("read length is bounded by the destination buffer");
            self.bytes.read(&mut buffer[..count])
        }
    }

    fn metadata(name: &str) -> SaveMetadata {
        fallback_metadata(SaveId::new("test"), SaveKind::Named, name.into(), 42)
    }

    #[test]
    fn metadata_and_payload_round_trip() {
        let metadata = metadata("Iron Works");
        let bytes = encode_container(&metadata, b"FACTSIM\0payload").unwrap();
        let (decoded, payload) = decode_container(&bytes).unwrap();
        assert_eq!(decoded, metadata);
        assert_eq!(payload, b"FACTSIM\0payload");
    }

    #[test]
    fn metadata_limit_is_enforced() {
        let error =
            encode_container(&metadata(&"x".repeat(MAX_METADATA_BYTES)), b"payload").unwrap_err();
        assert!(matches!(error, ContainerError::MetadataTooLarge(_)));
    }

    #[test]
    fn inspection_distinguishes_short_files_from_transient_read_failures() {
        let mut buffer = [0; 1];
        assert!(matches!(
            read_inspection_bytes(&mut io::Cursor::new([]), &mut buffer),
            Err(ContainerError::Truncated)
        ));

        let error = read_inspection_bytes(
            &mut FailingReader(io::ErrorKind::PermissionDenied),
            &mut buffer,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ContainerError::Io(error) if error.kind() == io::ErrorKind::PermissionDenied
        ));
    }

    #[test]
    fn simulation_payload_round_trip_preserves_tick_and_state_hash() {
        let mut simulation = Simulation::new_test_world(77);
        for _ in 0..12 {
            simulation.tick();
        }
        let expected = (simulation.tick_count(), simulation.state_hash());
        let payload = save_to_bytes(&simulation).unwrap();
        let bytes = encode_container(&metadata("Round Trip"), &payload).unwrap();
        let (_, payload) = decode_container(&bytes).unwrap();
        let loaded = load_from_bytes(payload).unwrap();
        assert_eq!((loaded.tick_count(), loaded.state_hash()), expected);
    }

    #[test]
    fn streamed_container_matches_byte_format_and_loads_without_payload_buffer() {
        let mut simulation = Simulation::new_test_world(78);
        for _ in 0..12 {
            simulation.tick();
        }
        let snapshot = try_capture_save_snapshot(&simulation, 3).unwrap();
        let expected_payload = save_snapshot_records_to_bytes(&snapshot).unwrap();
        let root = std::env::temp_dir().join(format!(
            "factory-container-stream-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-stream.factsim");
        let metadata = metadata("Streamed");
        let metrics = write_save_snapshot(
            &path,
            &metadata,
            &snapshot,
            &AtomicBool::new(false),
            &AtomicU8::new(SAVE_CANCEL_ACTIVE),
        )
        .unwrap()
        .unwrap();
        assert_eq!(metrics.simulation_bytes, expected_payload.len());
        let bytes = fs::read(&path).unwrap();
        let (decoded_metadata, payload) = decode_container(&bytes).unwrap();
        assert_eq!(decoded_metadata, metadata);
        assert_eq!(payload, expected_payload);
        // Production saves now carry the indexed record container.
        assert_eq!(&payload[..8], &RECORD_MAGIC);
        let loaded = load_simulation(&path).unwrap();
        assert_eq!(loaded.state_hash(), simulation.state_hash());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_container_is_a_self_contained_portable_export() {
        let mut simulation = Simulation::new_test_world(80);
        for _ in 0..12 {
            simulation.tick();
        }
        let snapshot = try_capture_save_snapshot(&simulation, 5).unwrap();
        let root = std::env::temp_dir().join(format!(
            "factory-container-copy-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-copy.factsim");
        let exported = root.join("exported-copy.factsim");
        let metrics = write_save_snapshot(
            &path,
            &metadata("Copied"),
            &snapshot,
            &AtomicBool::new(false),
            &AtomicU8::new(SAVE_CANCEL_ACTIVE),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            metrics.simulation_bytes,
            save_snapshot_records_to_bytes(&snapshot).unwrap().len()
        );
        // Export is a plain file copy across the transport/commit boundary.
        fs::copy(&path, &exported).unwrap();
        let original = load_simulation(&path).unwrap();
        let duplicate = load_simulation(&exported).unwrap();
        assert_eq!(duplicate.state_hash(), simulation.state_hash());
        assert_eq!(duplicate.tick_count(), original.tick_count());
        assert_eq!(duplicate.state_hash(), original.state_hash());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn record_payload_loads_under_partitioned_allowance() {
        let mut simulation = Simulation::new_test_world(82);
        for _ in 0..12 {
            simulation.tick();
        }
        let snapshot = try_capture_save_snapshot(&simulation, 11).unwrap();
        let payload = save_snapshot_records_to_bytes(&snapshot).unwrap();
        let bytes = encode_container(&metadata("Partitioned"), &payload).unwrap();
        let offset = container_payload_offset(&bytes).unwrap();
        let index = inspect_record_index(&payload).unwrap();
        let biggest = index
            .records
            .iter()
            .map(|record| record.encoded_len)
            .max()
            .expect("records exist");
        let decoded_total: u64 = index.records.iter().map(|record| record.decoded_len).sum();
        let limits = SaveLimits {
            max_encoded_bytes: bytes.len() as u64,
            max_decoded_bytes: decoded_total,
            max_record_bytes: biggest,
            ..SaveLimits::default()
        };
        // The legacy monolithic allowance cannot hold the framed payload,
        // while the partitioned allowance can.
        assert!((bytes.len() - offset) as u64 > limits.max_simulation_bytes());
        let loaded = load_simulation_from_reader(&mut io::Cursor::new(&bytes), limits).unwrap();
        assert_eq!(loaded.state_hash(), simulation.state_hash());
    }

    #[test]
    fn record_payload_encodes_under_partitioned_allowance() {
        let mut simulation = Simulation::new_test_world(83);
        for _ in 0..12 {
            simulation.tick();
        }
        let snapshot = try_capture_save_snapshot(&simulation, 13).unwrap();
        let payload = save_snapshot_records_to_bytes(&snapshot).unwrap();
        let index = inspect_record_index(&payload).unwrap();
        let biggest = index
            .records
            .iter()
            .map(|record| record.encoded_len)
            .max()
            .expect("records exist");
        let decoded_total: u64 = index.records.iter().map(|record| record.decoded_len).sum();
        let metadata = metadata("PartitionedEncode");
        let offset = (PREFIX_SIZE + ron::ser::to_string(&metadata).unwrap().len()) as u64;
        let limits = SaveLimits {
            max_encoded_bytes: offset + payload.len() as u64,
            max_decoded_bytes: decoded_total,
            max_record_bytes: biggest,
            ..SaveLimits::default()
        };
        // The framed payload exceeds the legacy monolithic allowance but fits
        // the artifact budget.
        assert!(payload.len() as u64 > limits.max_simulation_bytes());
        let bytes = encode_container_with_limits(&metadata, &payload, limits).unwrap();
        assert_eq!(bytes.len() as u64, offset + payload.len() as u64);
    }

    #[test]
    fn record_framing_matches_container_allowance_exactly() {
        let mut simulation = Simulation::new_test_world(81);
        for _ in 0..12 {
            simulation.tick();
        }
        let snapshot = try_capture_save_snapshot(&simulation, 9).unwrap();
        let root = std::env::temp_dir().join(format!(
            "factory-container-framing-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let reference = root.join("reference.factsim");
        write_save_snapshot(
            &reference,
            &metadata("Framing"),
            &snapshot,
            &AtomicBool::new(false),
            &AtomicU8::new(SAVE_CANCEL_ACTIVE),
        )
        .unwrap()
        .unwrap();
        let total = fs::read(&reference).unwrap().len() as u64;
        // The record header and manifest live inside the payload allowance,
        // so the exact artifact size must be accepted.
        let exact = SaveLimits {
            max_encoded_bytes: total,
            ..SaveLimits::default()
        };
        let bounded = root.join("bounded.factsim");
        let cancel = AtomicU8::new(SAVE_CANCEL_ACTIVE);
        write_save_snapshot_locked(
            &bounded,
            &metadata("Framing"),
            &snapshot,
            exact,
            &cancel,
            &CommitFaults::none(),
        )
        .unwrap();
        assert_eq!(fs::read(&bounded).unwrap().len() as u64, total);
        let short = SaveLimits {
            max_encoded_bytes: total - 1,
            ..SaveLimits::default()
        };
        assert!(
            write_save_snapshot_locked(
                &root.join("short.factsim"),
                &metadata("Framing"),
                &snapshot,
                short,
                &cancel,
                &CommitFaults::none(),
            )
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn payload_io_failure_remains_inaccessible_after_headers_succeed() {
        let simulation = Simulation::new_test_world(79);
        let payload = save_to_bytes(&simulation).unwrap();
        let bytes = encode_container(&metadata("I/O Failure"), &payload).unwrap();
        let fail_at = container_payload_offset(&bytes).unwrap() + SAVE_HEADER_SIZE + 16;
        assert!(fail_at < bytes.len());
        let mut reader = FailAfterReader {
            bytes: io::Cursor::new(bytes),
            fail_at: fail_at as u64,
            kind: io::ErrorKind::PermissionDenied,
        };

        assert!(matches!(
            load_simulation_from_reader(&mut reader, SaveLimits::default()),
            Err(ContainerError::Io(error))
                if error.kind() == io::ErrorKind::PermissionDenied
        ));
    }

    #[test]
    fn atomic_writer_creates_and_replaces_one_file() {
        let root = std::env::temp_dir().join(format!(
            "factory-container-atomic-{}-{}",
            std::process::id(),
            crate::save_load::catalog::now_unix_ms()
        ));
        let path = root.join("manual-test.factsim");
        write_save_bytes(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        write_save_bytes(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleted_paths_reclaim_epoch_entries() {
        let root = std::env::temp_dir().join(format!(
            "factory-epoch-reclaim-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("reclaimed.factsim");
        write_save_bytes(&path, b"payload").unwrap();
        assert!(SAVE_ARTIFACT_EPOCHS.lock().unwrap().contains_key(&path));
        let committed = save_artifact_epoch(&path);
        remove_save_and_artifacts(&path).unwrap();
        assert!(
            !SAVE_ARTIFACT_EPOCHS.lock().unwrap().contains_key(&path),
            "deletion must reclaim the path entry"
        );
        // Recreating the path draws a fresh commit-sequence value that
        // cannot realign with the pre-delete epoch: no ABA.
        write_save_bytes(&path, b"payload").unwrap();
        assert_ne!(committed, save_artifact_epoch(&path));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_stream_never_replaces_the_previous_save() {
        let root = std::env::temp_dir().join(format!(
            "factory-container-failed-stream-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-test.factsim");
        write_save_bytes(&path, b"previous valid save").unwrap();

        let cancel = AtomicU8::new(SAVE_CANCEL_ACTIVE);
        let result: Result<((), SaveDurability), ContainerError> = write_temporary_and_commit(
            &path,
            |writer| {
                writer.write_all(b"partial replacement")?;
                Err(ContainerError::Io(io::Error::other(
                    "injected encoder failure",
                )))
            },
            &cancel,
            &CommitFaults::none(),
        );
        assert!(matches!(result, Err(ContainerError::Io(_))));
        assert_eq!(fs::read(&path).unwrap(), b"previous valid save");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_stream_never_replaces_the_previous_save() {
        let mut simulation = Simulation::new_test_world(78);
        for _ in 0..12 {
            simulation.tick();
        }
        let snapshot = try_capture_save_snapshot(&simulation, 3).unwrap();
        let root = std::env::temp_dir().join(format!(
            "factory-container-cancelled-stream-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-test.factsim");
        let idle = AtomicBool::new(false);
        let active = AtomicU8::new(SAVE_CANCEL_ACTIVE);
        write_save_snapshot(&path, &metadata("Original"), &snapshot, &idle, &active)
            .unwrap()
            .unwrap();
        let previous = fs::read(&path).unwrap();
        // A cancel racing the encode is still honored at the commit point:
        // the flag is already set here, which exercises the same atomic
        // claim a mid-encode cancel hits before the rename.
        let cancel = AtomicU8::new(crate::save_load::lifecycle::SAVE_CANCEL_REQUESTED);
        let result =
            write_save_snapshot(&path, &metadata("Replacement"), &snapshot, &idle, &cancel);
        assert!(matches!(result, Some(Err(ContainerError::Cancelled))));
        assert_eq!(
            fs::read(&path).unwrap(),
            previous,
            "a cancelled save must leave the previous save intact"
        );
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            1,
            "a cancelled save must remove its temporary artifact"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn artifact_naming_round_trips_active_and_retired_paths() {
        let primary = Path::new("manual-test.factsim");
        let backup = save_artifact_path(primary, SaveArtifactKind::Backup, "12-ab");
        assert_eq!(
            parse_save_artifact(&backup),
            Some((primary.to_path_buf(), SaveArtifactKind::Backup))
        );
        let retired = backup.with_file_name(format!(
            "{}{}",
            backup.file_name().unwrap().to_string_lossy(),
            RETIRED_ARTIFACT_SUFFIX
        ));
        assert_eq!(
            retired_save_artifact_primary(&retired),
            Some(primary.to_path_buf())
        );
        assert!(parse_save_artifact(Path::new("manual-test.factsim.bak-12.bad")).is_none());
    }

    #[test]
    fn bounded_reader_and_writer_agree_at_boundary_and_preserve_primary() {
        let sim = Simulation::new_test_world(292);
        let payload = save_to_bytes(&sim).unwrap();
        let meta = metadata("Bounded");
        let bytes = encode_container(&meta, &payload).unwrap();
        let limits = SaveLimits {
            max_encoded_bytes: bytes.len() as u64,
            max_decoded_bytes: (payload.len() - SAVE_HEADER_SIZE) as u64,
            ..SaveLimits::default()
        };
        assert_eq!(
            encode_container_with_limits(&meta, &payload, limits).unwrap(),
            bytes
        );
        let root = std::env::temp_dir().join(format!(
            "factory-save-limits-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-boundary.factsim");
        with_save_artifact_lock(|| {
            write_save_bytes_locked(&path, &bytes, limits, &CommitFaults::none()).unwrap();
            assert_eq!(
                load_simulation_from_reader(&mut fs::File::open(&path).unwrap(), limits)
                    .unwrap()
                    .state_hash(),
                sim.state_hash()
            );
            let mut oversized = bytes.clone();
            oversized.push(0);
            assert!(matches!(
                write_save_bytes_locked(&path, &oversized, limits, &CommitFaults::none()),
                Err(ContainerError::TooLarge)
            ));
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
            let mut reader = io::Cursor::new(&oversized);
            assert!(matches!(
                load_simulation_from_reader(&mut reader, limits),
                Err(ContainerError::TooLarge)
            ));
            assert_eq!(reader.position(), bytes.len() as u64 + 1);
        });
        fs::remove_dir_all(root).unwrap();
        let raw_limits = SaveLimits {
            max_encoded_bytes: payload.len() as u64,
            ..limits
        };
        assert_eq!(
            load_simulation_from_reader(&mut io::Cursor::new(&payload), raw_limits)
                .unwrap()
                .state_hash(),
            sim.state_hash()
        );
        let mut raw = payload;
        raw.push(0);
        assert!(matches!(
            load_simulation_from_reader(&mut io::Cursor::new(raw), raw_limits),
            Err(ContainerError::TooLarge)
        ));
    }

    #[test]
    fn world_seed_metadata_round_trips_full_u64() {
        for seed in [0, 123, u64::MAX] {
            let mut with_seed = metadata("Seeded");
            with_seed.world_seed = Some(seed);
            assert_eq!(with_seed.schema_version, METADATA_SCHEMA_VERSION);
            let bytes = encode_container(&with_seed, b"FACTSIM\0payload").unwrap();
            let (decoded, payload) = decode_container(&bytes).unwrap();
            assert_eq!(decoded.world_seed, Some(seed));
            assert_eq!(decoded, with_seed);
            assert_eq!(payload, b"FACTSIM\0payload");
            let reparsed: SaveMetadata =
                ron::de::from_str(&ron::ser::to_string(&with_seed).unwrap()).unwrap();
            assert_eq!(reparsed.world_seed, Some(seed));
        }
    }

    #[test]
    fn legacy_metadata_without_seed_decodes_to_unknown() {
        let legacy = concat!(
            "(schema_version: 1, id: \"manual-legacy\", ",
            "display_name: \"Legacy\", kind: Named, ",
            "completed_at_unix_ms: 42, application_version: \"0.1.0\")",
        );
        let metadata_len = legacy.len() as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&CONTAINER_MAGIC);
        bytes.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&metadata_len.to_le_bytes());
        bytes.extend_from_slice(legacy.as_bytes());
        bytes.extend_from_slice(b"FACTSIM\0payload");
        let (decoded, payload) = decode_container(&bytes).unwrap();
        assert_eq!(decoded.world_seed, None);
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.world_seed_label(), "Seed unknown");
        assert_eq!(payload, b"FACTSIM\0payload");
    }

    #[test]
    fn try_acquire_defers_instead_of_blocking_on_held_lock() {
        // Frame-side installation must never stall on the artifact lock:
        // when a save worker or scan holds it, acquisition fails fast so
        // the candidate is retained and rechecked next frame under the lock.
        let _held = hold_save_artifact_lock_for_tests();
        assert!(
            try_acquire_save_artifact_lock().is_none(),
            "installation must defer while the artifact lock is held"
        );
        drop(_held);
        // Other test threads may briefly hold the process-wide lock (fault
        // matrix, concurrent scans); poll until it frees instead of
        // asserting on a single racy acquisition.
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if try_acquire_save_artifact_lock().is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "installation must proceed once the artifact lock is free"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn forged_prefixes_fail_before_reading_declared_data() {
        let mut bytes = CONTAINER_MAGIC.to_vec();
        bytes.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            load_simulation_from_reader(&mut io::Cursor::new(&bytes), SaveLimits::default()),
            Err(ContainerError::MetadataTooLarge(_))
        ));
        bytes[12..16].copy_from_slice(&10_u32.to_le_bytes());
        assert!(matches!(
            load_simulation_from_reader(&mut io::Cursor::new(&bytes), SaveLimits::default()),
            Err(ContainerError::Truncated)
        ));
        bytes[8..12].copy_from_slice(&(CONTAINER_VERSION + 1).to_le_bytes());
        assert!(matches!(
            load_simulation_from_reader(&mut io::Cursor::new(&bytes), SaveLimits::default()),
            Err(ContainerError::UnsupportedVersion(_))
        ));
        assert!(matches!(
            decode_container(&bytes),
            Err(ContainerError::UnsupportedVersion(_))
        ));
        assert!(matches!(
            load_simulation_from_reader(
                &mut FailingReader(io::ErrorKind::PermissionDenied),
                SaveLimits::default()
            ),
            Err(ContainerError::Io(_))
        ));
    }

    fn fault_test_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "factory-commit-fault-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn valid_container_bytes(seed: u64, ticks: usize, name: &str) -> Vec<u8> {
        let mut simulation = Simulation::new_test_world(seed);
        for _ in 0..ticks {
            simulation.tick();
        }
        let payload = save_to_bytes(&simulation).unwrap();
        encode_container(&metadata(name), &payload).unwrap()
    }

    fn valid_quicksave_bytes(seed: u64, ticks: usize) -> Vec<u8> {
        let mut simulation = Simulation::new_test_world(seed);
        for _ in 0..ticks {
            simulation.tick();
        }
        let payload = save_to_bytes(&simulation).unwrap();
        let quicksave_metadata = SaveMetadata {
            schema_version: METADATA_SCHEMA_VERSION,
            id: SaveId::new("quicksave"),
            display_name: "Quicksave".into(),
            kind: SaveKind::Quicksave,
            completed_at_unix_ms: 42,
            application_version: env!("CARGO_PKG_VERSION").into(),
            world_seed: None,
        };
        encode_container(&quicksave_metadata, &payload).unwrap()
    }

    #[test]
    fn commit_faults_before_rename_preserve_previous_save() {
        use std::io::ErrorKind;

        let phases = [
            CommitFaultPhase::CreateDir,
            CommitFaultPhase::CreateTemp,
            CommitFaultPhase::Write,
            CommitFaultPhase::Flush,
            CommitFaultPhase::SyncTemp,
            CommitFaultPhase::SyncParentPre,
            CommitFaultPhase::Backup,
            CommitFaultPhase::Rename,
        ];
        // Disk-full, permission/locked, and partial I/O share the pre-commit
        // contract: the previous save stays intact and no artifact leaks.
        let kinds = [
            ErrorKind::StorageFull,
            ErrorKind::PermissionDenied,
            ErrorKind::Other,
        ];
        for phase in phases {
            for kind in kinds {
                let root = fault_test_root(&format!("{phase:?}-{kind:?}"));
                let path = root.join("manual-test.factsim");
                write_save_bytes(&path, b"previous valid save").unwrap();
                let faults = CommitFaults::fail_at(phase, kind);
                let result = write_save_bytes_with_faults(&path, b"new save", &faults);
                assert!(
                    matches!(result, Err(ContainerError::Io(_))),
                    "phase {phase:?} with {kind:?} must fail pre-commit, got {result:?}"
                );
                assert_eq!(
                    fs::read(&path).unwrap(),
                    b"previous valid save",
                    "phase {phase:?} with {kind:?} must leave the previous save intact"
                );
                assert_eq!(
                    fs::read_dir(&root).unwrap().count(),
                    1,
                    "phase {phase:?} with {kind:?} must not leak temporary artifacts"
                );
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn sync_barrier_failure_still_commits_with_degraded_durability() {
        use std::io::ErrorKind;

        for kind in [ErrorKind::PermissionDenied, ErrorKind::StorageFull] {
            let root = fault_test_root(&format!("barrier-{kind:?}"));
            let path = root.join("manual-test.factsim");
            write_save_bytes(&path, b"previous").unwrap();
            let faults = CommitFaults::fail_at(CommitFaultPhase::SyncInstalled, kind);
            let durability = write_save_bytes_with_faults(&path, b"committed", &faults)
                .expect("a failed durability barrier must still commit");
            assert_eq!(
                durability.degraded_reason(),
                Some("injected commit fault"),
                "barrier failure with {kind:?} must degrade, not read as durable"
            );
            assert_eq!(
                fs::read(&path).unwrap(),
                b"committed",
                "barrier failure must not roll back the committed save"
            );
            assert_eq!(
                fs::read_dir(&root).unwrap().count(),
                1,
                "committed save must clean its temporary artifact"
            );
            // The degraded commit is a success, never an error that would
            // invite an unsafe overwrite retry.
            let message = crate::save_load::commit::format_save_success("Quicksave", &durability);
            assert!(
                message.contains("durability is degraded"),
                "degraded commit must not read as fully durable: {message}"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn retire_failure_keeps_committed_save_for_next_scan() {
        let root = fault_test_root("retire");
        let path = root.join("manual-test.factsim");
        let old_bytes = valid_container_bytes(11, 4, "Old");
        let new_bytes = valid_container_bytes(11, 8, "New");
        assert_ne!(old_bytes, new_bytes);
        write_save_bytes(&path, &old_bytes).unwrap();

        let faults = CommitFaults::fail_at(
            CommitFaultPhase::Retire,
            std::io::ErrorKind::PermissionDenied,
        );
        let durability = write_save_bytes_with_faults(&path, &new_bytes, &faults)
            .expect("retirement failure must not fail a committed save");
        assert_eq!(durability.degraded_reason(), None);
        assert_eq!(
            fs::read(&path).unwrap(),
            new_bytes,
            "the committed save wins even when retirement fails"
        );
        // The rollback backup remains for the next catalog scan, which drops
        // it because the primary is valid — the save is never rolled back.
        let config = crate::save_load::SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), new_bytes);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn post_cleanup_sync_failure_keeps_committed_save() {
        let root = fault_test_root("sync-parent-post");
        let path = root.join("manual-test.factsim");
        let old_bytes = valid_container_bytes(12, 4, "Old");
        let new_bytes = valid_container_bytes(12, 8, "New");
        assert_ne!(old_bytes, new_bytes);
        write_save_bytes(&path, &old_bytes).unwrap();

        let faults = CommitFaults::fail_at(
            CommitFaultPhase::SyncParentPost,
            std::io::ErrorKind::StorageFull,
        );
        let durability = write_save_bytes_with_faults(&path, &new_bytes, &faults)
            .expect("post-cleanup sync failure must not fail a committed save");
        assert_eq!(durability.degraded_reason(), None);
        assert_eq!(
            fs::read(&path).unwrap(),
            new_bytes,
            "the committed save wins even when post-cleanup sync fails"
        );
        let config = crate::save_load::SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), new_bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn first_save_in_new_root_reports_durable() {
        let outer = fault_test_root("new-root");
        let path = outer.join("brand").join("new").join("manual-test.factsim");
        assert!(!outer.join("brand").exists());
        let durability =
            write_save_bytes(&path, b"first generation").expect("first save must commit");
        assert_eq!(durability.degraded_reason(), None);
        assert_eq!(fs::read(&path).unwrap(), b"first generation");
        fs::remove_dir_all(outer).unwrap();
    }

    #[test]
    fn ancestor_sync_failures_degrade_instead_of_reading_durable() {
        // The barrier must sync every created directory plus the linking
        // parent; any failure degrades rather than reading as durable.
        assert!(sync_created_ancestors(&[], None).is_ok());
        let missing = std::env::temp_dir().join(format!(
            "factory-no-such-dir-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        assert!(!missing.exists());
        assert!(
            sync_created_ancestors(std::slice::from_ref(&missing), None).is_err(),
            "an unsyncable ancestor must degrade, never read as durable"
        );
    }

    #[test]
    fn cancel_cannot_remove_the_only_recoverable_backup() {
        use crate::save_load::SaveLoadConfig;

        let root = fault_test_root("cancel-recoverable");
        let path = root.join("quicksave.factsim");
        let backup_bytes = valid_quicksave_bytes(21, 4);
        // Crash state: no primary, exactly one valid backup.
        let backup_path = path.with_file_name(format!(
            "quicksave.factsim{}crash-cancel",
            BACKUP_ARTIFACT_MARKER
        ));
        fs::write(&backup_path, &backup_bytes).unwrap();
        assert!(!path.exists());

        // A new save cancelled before its commit point must remove only its
        // own temporary artifact, never the pre-existing recoverable backup.
        let cancel = AtomicU8::new(crate::save_load::lifecycle::SAVE_CANCEL_REQUESTED);
        let snapshot = try_capture_save_snapshot(&Simulation::new_test_world(21), 1).unwrap();
        let shutdown = AtomicBool::new(false);
        let result = write_save_snapshot_with_faults(
            &path,
            &metadata("Replacement"),
            &snapshot,
            &shutdown,
            &cancel,
            &CommitFaults::none(),
        );
        assert!(matches!(result, Some(Err(ContainerError::Cancelled))));
        assert!(
            backup_path.exists(),
            "cancellation must preserve the only recoverable backup"
        );
        assert!(!path.exists());

        // Recovery still promotes the preserved backup.
        let config = SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), backup_bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_scan_during_commit_preserves_one_generation() {
        use crate::save_load::SaveLoadConfig;

        let root = fault_test_root("concurrent-scan");
        let path = root.join("quicksave.factsim");
        let old_bytes = valid_quicksave_bytes(31, 4);
        let new_bytes = valid_quicksave_bytes(31, 9);
        write_save_bytes(&path, &old_bytes).unwrap();
        let config = SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };

        let scan_config = config.clone();
        let scanner = std::thread::spawn(move || {
            for _ in 0..8 {
                let _ = crate::save_load::scan_catalog(&scan_config);
            }
        });
        for _ in 0..4 {
            write_save_bytes(&path, &new_bytes).unwrap();
            write_save_bytes(&path, &old_bytes).unwrap();
        }
        scanner.join().unwrap();

        // The scan worker and the writer serialize on the artifact lock, so
        // the directory never exposes a mixture: the primary is exactly one
        // complete generation and decodes.
        let primary = fs::read(&path).unwrap();
        assert!(
            primary == old_bytes || primary == new_bytes,
            "concurrent scan must never mix snapshot generations"
        );
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), primary);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_preserves_ambiguous_backups_and_discards_corrupt() {
        use crate::save_load::SaveLoadConfig;

        let root = fault_test_root("ambiguous");
        let path = root.join("quicksave.factsim");
        let valid_bytes = valid_quicksave_bytes(41, 4);
        write_save_bytes(&path, &valid_bytes).unwrap();

        // A corrupt backup next to a valid primary is discarded.
        let corrupt = path.with_file_name(format!(
            "quicksave.factsim{BACKUP_ARTIFACT_MARKER}corrupt-1"
        ));
        fs::write(&corrupt, b"invalid backup").unwrap();
        let config = SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), valid_bytes);
        assert!(!corrupt.exists());

        // Two different valid backups with a corrupt primary are ambiguous:
        // recovery preserves both instead of guessing.
        let other_bytes = valid_quicksave_bytes(41, 9);
        assert_ne!(valid_bytes, other_bytes);
        fs::write(&path, b"corrupt primary").unwrap();
        let backup_one = path.with_file_name(format!(
            "quicksave.factsim{BACKUP_ARTIFACT_MARKER}ambiguous-1"
        ));
        let backup_two = path.with_file_name(format!(
            "quicksave.factsim{BACKUP_ARTIFACT_MARKER}ambiguous-2"
        ));
        fs::write(&backup_one, &valid_bytes).unwrap();
        fs::write(&backup_two, &other_bytes).unwrap();
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].compatibility.can_load());
        assert_eq!(fs::read(&path).unwrap(), b"corrupt primary");
        assert!(backup_one.exists() && backup_two.exists());

        // Removing one candidate resolves the ambiguity: the survivor promotes.
        fs::remove_file(&backup_two).unwrap();
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), valid_bytes);

        // Identical duplicates are deduplicated: two copies of one valid
        // backup with a corrupt primary still promote exactly that
        // generation instead of preserving a false ambiguity.
        fs::write(&path, b"corrupt primary").unwrap();
        fs::write(&backup_one, &valid_bytes).unwrap();
        fs::write(&backup_two, &valid_bytes).unwrap();
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read(&path).unwrap(), valid_bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deletion_cannot_resurrect_an_old_generation() {
        use crate::save_load::SaveLoadConfig;

        let root = fault_test_root("delete-no-resurrect");
        let path = root.join("quicksave.factsim");
        let bytes = valid_quicksave_bytes(51, 4);
        write_save_bytes(&path, &bytes).unwrap();
        let backup =
            path.with_file_name(format!("quicksave.factsim{BACKUP_ARTIFACT_MARKER}doomed-1"));
        let temporary =
            path.with_file_name(format!("quicksave.factsim{TEMP_ARTIFACT_MARKER}doomed-1"));
        fs::write(&backup, &bytes).unwrap();
        fs::write(&temporary, &bytes).unwrap();

        remove_save_and_artifacts(&path).unwrap();
        assert!(!path.exists());
        assert!(!backup.exists());
        assert!(!temporary.exists());

        let config = SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let entries = crate::save_load::scan_catalog(&config).unwrap();
        assert!(
            entries.is_empty(),
            "intentional deletion must not resurrect an old generation"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
