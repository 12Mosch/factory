use super::{SaveId, SaveKind, SaveMetadata};
use factory_sim::{
    SAVE_HEADER_SIZE, SaveLimits, SaveLoadError, Simulation, SimulationSaveSnapshot,
    load_from_reader_with_limits, save_snapshot_to_writer,
};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::{fs, str};

static SAVE_ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(0);
static SAVE_ARTIFACT_LOCK: Mutex<()> = Mutex::new(());

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
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StreamWriteMetrics {
    pub total_bytes: usize,
    pub simulation_bytes: usize,
    pub encode_ms: f64,
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
    check_size(payload.len() as u64, limits.max_simulation_bytes())?;
    let metadata_text = ron::ser::to_string(metadata)
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    let metadata_bytes = metadata_text.as_bytes();
    if metadata_bytes.len() > limits.max_metadata_bytes {
        return Err(ContainerError::MetadataTooLarge(metadata_bytes.len()));
    }
    let metadata_len = u32::try_from(metadata_bytes.len())
        .map_err(|_| ContainerError::MetadataTooLarge(metadata_bytes.len()))?;
    check_size(
        (PREFIX_SIZE + metadata_bytes.len()) as u64 + payload.len() as u64,
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

/// Decodes container metadata and returns a borrowed simulation payload.
pub fn decode_container(bytes: &[u8]) -> Result<(SaveMetadata, &[u8]), ContainerError> {
    let payload_offset = container_payload_offset(bytes)?;
    check_size(bytes.len() as u64, SaveLimits::default().max_encoded_bytes)?;
    check_size(
        (bytes.len() - payload_offset) as u64,
        SaveLimits::default().max_simulation_bytes(),
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
fn read_inspection_bytes(reader: &mut impl Read, buffer: &mut [u8]) -> Result<(), ContainerError> {
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
    let maximum = limits
        .max_simulation_bytes()
        .min(limits.max_encoded_bytes - overhead);
    let mut payload = reader.take(maximum.saturating_add(1));
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

/// Serializes save-directory mutations across the catalog and background writer.
pub(crate) fn with_save_artifact_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = SAVE_ARTIFACT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operation()
}

/// Writes and durably installs a complete save without exposing partial contents.
pub(crate) fn write_save_bytes(path: &Path, bytes: &[u8]) -> Result<(), ContainerError> {
    with_save_artifact_lock(|| write_save_bytes_locked(path, bytes, SaveLimits::default()))
}

/// Encodes a snapshot through a buffered temporary file and commits it only
/// after the encoder has finished and the buffer has been flushed and synced.
pub(crate) fn write_save_snapshot(
    path: &Path,
    metadata: &SaveMetadata,
    snapshot: &SimulationSaveSnapshot,
) -> Result<StreamWriteMetrics, ContainerError> {
    with_save_artifact_lock(|| {
        write_save_snapshot_locked(path, metadata, snapshot, SaveLimits::default())
    })
}

fn write_save_snapshot_locked(
    path: &Path,
    metadata: &SaveMetadata,
    snapshot: &SimulationSaveSnapshot,
    limits: SaveLimits,
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
    let payload_maximum = limits
        .max_simulation_bytes()
        .min(limits.max_encoded_bytes - overhead);

    write_temporary_and_commit(path, |writer| {
        writer.write_all(&CONTAINER_MAGIC)?;
        writer.write_all(&CONTAINER_VERSION.to_le_bytes())?;
        writer.write_all(&metadata_len.to_le_bytes())?;
        writer.write_all(metadata_bytes)?;
        let encode_start = Instant::now();
        let simulation_bytes = {
            let mut payload = LimitedWriter::new(writer, payload_maximum);
            save_snapshot_to_writer(snapshot, &mut payload).map_err(map_simulation_error)?;
            payload.written
        };
        Ok(StreamWriteMetrics {
            total_bytes: overhead as usize + simulation_bytes,
            simulation_bytes,
            encode_ms: encode_start.elapsed().as_secs_f64() * 1000.0,
        })
    })
}

/// Implements save installation while the process-wide artifact lock is held.
fn write_save_bytes_locked(
    path: &Path,
    bytes: &[u8],
    limits: SaveLimits,
) -> Result<(), ContainerError> {
    write_temporary_and_commit(path, |temp| {
        check_size(bytes.len() as u64, limits.max_encoded_bytes)?;
        if bytes.starts_with(&CONTAINER_MAGIC) {
            let offset = container_payload_offset_with_limits(bytes, limits)?;
            check_size((bytes.len() - offset) as u64, limits.max_simulation_bytes())?;
        } else {
            check_size(bytes.len() as u64, limits.max_simulation_bytes())?;
        }
        temp.write_all(bytes)?;
        Ok(())
    })
}

fn write_temporary_and_commit<T>(
    path: &Path,
    encode: impl FnOnce(&mut BufWriter<fs::File>) -> Result<T, ContainerError>,
) -> Result<T, ContainerError> {
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
        let temp = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        let mut temp = BufWriter::new(temp);
        let outcome = encode(&mut temp)?;
        temp.flush()?;
        temp.get_ref().sync_all()?;
        drop(temp);
        sync_parent_directory(path)?;

        let replaced = commit_temporary_file(path, &temp_path, &backup_path)?;
        installed = true;
        // Installation has committed. A durability-barrier failure must not be
        // reported as a failed save because retrying could overwrite success.
        let _ = sync_installed_file(path);

        if replaced {
            // The new primary is committed. Cleanup cannot turn that successful
            // save into an error; catalog refresh retries any leftover backup.
            let _ = discard_save_artifact(&backup_path);
            let _ = sync_parent_directory(path);
        }
        Ok(outcome)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
        if !installed {
            let _ = fs::remove_file(&backup_path);
        }
        let _ = sync_parent_directory(path);
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
            Err(_) if !path.try_exists()? => install_new_file(backup_path, path)?,
            Err(error) => return Err(error),
        }
    } else {
        install_new_file(backup_path, path)?;
    }
    // Promotion has committed even if a post-rename durability barrier is not
    // available on this filesystem or is temporarily blocked by another handle.
    let _ = sync_installed_file(path);
    Ok(())
}

/// Removes a canonical save and all of its recovery artifacts as one serialized
/// operation so an intentional deletion cannot be mistaken for a crashed write.
pub(crate) fn remove_save_and_artifacts(path: &Path) -> io::Result<()> {
    with_save_artifact_lock(|| {
        for artifact in save_artifacts_for(path)? {
            discard_save_artifact(&artifact)?;
        }
        sync_parent_directory(path)?;
        fs::remove_file(path)?;
        sync_parent_directory(path)
    })
}

/// Durably removes an artifact, first retiring a backup so cleanup failure can
/// never leave an old snapshot eligible for automatic recovery.
pub(crate) fn discard_save_artifact(path: &Path) -> io::Result<()> {
    if parse_save_artifact(path).is_some_and(|(_, kind)| kind == SaveArtifactKind::Backup) {
        retire_recovery_artifact(path)
    } else if path.try_exists()? {
        fs::remove_file(path)?;
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
fn retire_recovery_artifact(path: &Path) -> io::Result<()> {
    if !path.try_exists()? {
        return Ok(());
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        fs::remove_file(path)?;
        return sync_parent_directory(path);
    };
    let retired = path.with_file_name(format!("{file_name}{RETIRED_ARTIFACT_SUFFIX}"));
    match rename_file_no_replace(path, &retired) {
        Ok(()) => {
            sync_parent_directory(path)?;
            let _ = fs::remove_file(retired);
            let _ = sync_parent_directory(path);
            Ok(())
        }
        Err(rename_error) => match fs::remove_file(path) {
            Ok(()) => sync_parent_directory(path),
            Err(_) => Err(rename_error),
        },
    }
}

/// Installs a temporary file without a check-then-overwrite window.
fn commit_temporary_file(path: &Path, temp_path: &Path, backup_path: &Path) -> io::Result<bool> {
    if path.try_exists()? {
        replace_file(path, temp_path, backup_path)?;
        return Ok(true);
    }
    match install_new_file(temp_path, path) {
        Ok(()) => Ok(false),
        Err(_) if path.try_exists()? => {
            replace_file(path, temp_path, backup_path)?;
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

/// Creates a durable rollback link before atomically replacing the primary.
#[cfg(unix)]
fn replace_file(path: &Path, temp_path: &Path, backup_path: &Path) -> io::Result<()> {
    if fs::hard_link(path, backup_path).is_err() {
        fs::copy(path, backup_path)?;
        fs::File::open(backup_path)?.sync_all()?;
    }
    sync_parent_directory(path)?;
    fs::rename(temp_path, path)
}

/// Atomically replaces the primary and asks Windows to retain its old contents.
#[cfg(windows)]
fn replace_file(path: &Path, temp_path: &Path, backup_path: &Path) -> io::Result<()> {
    replace_file_windows(path, temp_path, Some(backup_path))
}

/// Portable fallback that copies the rollback snapshot before replacement.
#[cfg(not(any(unix, windows)))]
fn replace_file(path: &Path, temp_path: &Path, backup_path: &Path) -> io::Result<()> {
    fs::copy(path, backup_path)?;
    fs::File::open(backup_path)?.sync_all()?;
    sync_parent_directory(path)?;
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
fn install_new_file(temp_path: &Path, path: &Path) -> io::Result<()> {
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
    install_new_file(source, destination)
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
fn install_new_file(temp_path: &Path, path: &Path) -> io::Result<()> {
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
fn install_new_file(temp_path: &Path, path: &Path) -> io::Result<()> {
    fs::hard_link(temp_path, path)?;
    let _ = fs::remove_file(temp_path);
    Ok(())
}

/// Flushes containing-directory metadata with a directory fsync on Unix.
#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => fs::File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

/// Flushes Windows directory metadata through a backup-semantics handle.
#[cfg(windows)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::OpenOptions::new()
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(parent)?
        .sync_all()
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

/// Uses directory fsync as the installation durability barrier elsewhere.
#[cfg(not(windows))]
fn sync_installed_file(path: &Path) -> io::Result<()> {
    sync_parent_directory(path)
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
        Simulation, load_from_bytes, save_snapshot_to_bytes, save_to_bytes,
        try_capture_save_snapshot,
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
        let expected_payload = save_snapshot_to_bytes(&snapshot).unwrap();
        let root = std::env::temp_dir().join(format!(
            "factory-container-stream-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-stream.factsim");
        let metadata = metadata("Streamed");
        let metrics = write_save_snapshot(&path, &metadata, &snapshot).unwrap();
        assert_eq!(metrics.simulation_bytes, expected_payload.len());
        let bytes = fs::read(&path).unwrap();
        let (decoded_metadata, payload) = decode_container(&bytes).unwrap();
        assert_eq!(decoded_metadata, metadata);
        assert_eq!(payload, expected_payload);
        let loaded = load_simulation(&path).unwrap();
        assert_eq!(loaded.state_hash(), simulation.state_hash());
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
    fn failed_stream_never_replaces_the_previous_save() {
        let root = std::env::temp_dir().join(format!(
            "factory-container-failed-stream-{}-{}",
            std::process::id(),
            SAVE_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("manual-test.factsim");
        write_save_bytes(&path, b"previous valid save").unwrap();

        let result: Result<(), ContainerError> = write_temporary_and_commit(&path, |writer| {
            writer.write_all(b"partial replacement")?;
            Err(ContainerError::Io(io::Error::other(
                "injected encoder failure",
            )))
        });
        assert!(matches!(result, Err(ContainerError::Io(_))));
        assert_eq!(fs::read(&path).unwrap(), b"previous valid save");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
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
            write_save_bytes_locked(&path, &bytes, limits).unwrap();
            assert_eq!(
                load_simulation_from_reader(&mut fs::File::open(&path).unwrap(), limits)
                    .unwrap()
                    .state_hash(),
                sim.state_hash()
            );
            let mut oversized = bytes.clone();
            oversized.push(0);
            assert!(matches!(
                write_save_bytes_locked(&path, &oversized, limits),
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
}
