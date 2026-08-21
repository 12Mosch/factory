use super::{SaveId, SaveKind, SaveMetadata};
use factory_sim::SAVE_HEADER_SIZE;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, str};

static SAVE_ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(0);
static SAVE_ARTIFACT_LOCK: Mutex<()> = Mutex::new(());

/// Magic bytes at the start of a Factory save container.
pub const CONTAINER_MAGIC: [u8; 8] = *b"FACTSAVE";
/// Current Factory save-container format version.
pub const CONTAINER_VERSION: u32 = 1;
pub const METADATA_SCHEMA_VERSION: u32 = 1;
/// Maximum serialized metadata size accepted by the container parser.
pub const MAX_METADATA_BYTES: usize = 16 * 1024;
/// Marker separating a canonical save name from a temporary artifact nonce.
pub const TEMP_ARTIFACT_MARKER: &str = ".tmp-";
/// Marker separating a canonical save name from a backup artifact nonce.
pub const BACKUP_ARTIFACT_MARKER: &str = ".bak-";
const PREFIX_SIZE: usize = 16;
const RETIRED_ARTIFACT_SUFFIX: &str = ".retired";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveArtifactKind {
    Temporary,
    Backup,
}

/// Error produced while encoding, inspecting, or decoding a save container.
#[derive(Debug)]
pub enum ContainerError {
    Io(io::Error),
    MetadataTooLarge(usize),
    MetadataEncoding(String),
    Truncated,
    InvalidContainerMagic,
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::MetadataTooLarge(size) => write!(
                formatter,
                "metadata is {size} bytes (maximum is {MAX_METADATA_BYTES})"
            ),
            Self::MetadataEncoding(error) => write!(formatter, "metadata encoding failed: {error}"),
            Self::Truncated => write!(formatter, "save container is truncated"),
            Self::InvalidContainerMagic => write!(formatter, "invalid save container magic"),
        }
    }
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
    let metadata_text = ron::ser::to_string(metadata)
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    let metadata_bytes = metadata_text.as_bytes();
    if metadata_bytes.len() > MAX_METADATA_BYTES {
        return Err(ContainerError::MetadataTooLarge(metadata_bytes.len()));
    }
    let metadata_len = u32::try_from(metadata_bytes.len())
        .map_err(|_| ContainerError::MetadataTooLarge(metadata_bytes.len()))?;
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
    let metadata = ron::de::from_bytes(&bytes[PREFIX_SIZE..payload_offset])
        .map_err(|error| ContainerError::MetadataEncoding(error.to_string()))?;
    Ok((metadata, &bytes[payload_offset..]))
}

/// Validates the fixed prefix and computes the first payload byte.
fn container_payload_offset(bytes: &[u8]) -> Result<usize, ContainerError> {
    if bytes.len() < PREFIX_SIZE {
        return Err(ContainerError::Truncated);
    }
    if bytes[..8] != CONTAINER_MAGIC {
        return Err(ContainerError::InvalidContainerMagic);
    }
    let metadata_len = u32::from_le_bytes(bytes[12..16].try_into().expect("fixed range")) as usize;
    if metadata_len > MAX_METADATA_BYTES {
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
    let mut file = fs::File::open(path)?;
    let mut prefix = [0; PREFIX_SIZE];
    read_inspection_bytes(&mut file, &mut prefix)?;
    if prefix[..8] != CONTAINER_MAGIC {
        return Err(ContainerError::InvalidContainerMagic);
    }
    let version = u32::from_le_bytes(prefix[8..12].try_into().expect("fixed range"));
    let metadata_len = u32::from_le_bytes(prefix[12..16].try_into().expect("fixed range")) as usize;
    if metadata_len > MAX_METADATA_BYTES {
        return Err(ContainerError::MetadataTooLarge(metadata_len));
    }
    let mut metadata_bytes = vec![0; metadata_len];
    read_inspection_bytes(&mut file, &mut metadata_bytes)?;
    let metadata = ron::de::from_bytes(&metadata_bytes).ok();
    let mut simulation_header = vec![0; SAVE_HEADER_SIZE];
    read_inspection_bytes(&mut file, &mut simulation_header)?;
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

/// Reads a container payload, retaining support for legacy raw quicksaves.
pub(crate) fn read_simulation_payload(path: &Path) -> Result<Vec<u8>, ContainerError> {
    let bytes = fs::read(path)?;
    if bytes.starts_with(&CONTAINER_MAGIC) {
        let payload_offset = container_payload_offset(&bytes)?;
        Ok(bytes[payload_offset..].to_vec())
    } else {
        Ok(bytes)
    }
}

/// Serializes save-directory mutations across the catalog and background writer.
pub(crate) fn with_save_artifact_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = SAVE_ARTIFACT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operation()
}

/// Writes and durably installs a complete save without exposing partial contents.
pub(crate) fn write_save_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    with_save_artifact_lock(|| write_save_bytes_locked(path, bytes))
}

/// Implements save installation while the process-wide artifact lock is held.
fn write_save_bytes_locked(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
        let mut temp = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        temp.write_all(bytes)?;
        temp.sync_all()?;
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
        Ok(())
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_sim::{Simulation, load_from_bytes, save_to_bytes};

    struct FailingReader(io::ErrorKind);

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(self.0))
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
}
