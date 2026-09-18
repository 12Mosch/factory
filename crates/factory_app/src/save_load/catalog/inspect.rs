//! Lightweight save inspection: filename mapping, header classification,
//! and file fingerprints.
//!
//! Everything here is read-only and cheap: no payload is ever decoded.
//! Recovery, scanning, validation, and polling all build on these helpers,
//! so a classification is always bound to the file instance its handle
//! observed — never to a path that may have been replaced since.

use super::super::compatibility::classify_header;
use super::super::container::{
    CONTAINER_VERSION, ContainerError, inspect_container_from_reader, read_inspection_bytes,
};
use super::super::{
    SaveCompatibility, SaveFileFingerprint, SaveFileIdentity, SaveFileMetadataFingerprint, SaveId,
    SaveKind, SaveMetadata, local_datetime_from_unix_ms,
};
use factory_sim::{SAVE_HEADER_SIZE, inspect_save_header};
use std::fs;
use std::io::{Read, Seek};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct FileInspection {
    pub(crate) metadata: Option<SaveMetadata>,
    pub(crate) compatibility: SaveCompatibility,
    pub(crate) safe_to_replace: bool,
    pub(crate) inspected: Option<SaveFileMetadataFingerprint>,
}

/// Maps canonical file names to stable save identities and kinds.
pub(crate) fn recognized_file(
    path: &Path,
    autosave_count: usize,
) -> Option<(SaveId, SaveKind, String)> {
    let file_name = path.file_name()?.to_str()?;
    if file_name == "quicksave.factsim" {
        return Some((
            SaveId::new("quicksave"),
            SaveKind::Quicksave,
            "Quicksave".into(),
        ));
    }
    if let Some(number) = file_name
        .strip_prefix("autosave-")
        .and_then(|value| value.strip_suffix(".factsim"))
        .and_then(|value| value.parse::<usize>().ok())
    {
        if (1..=autosave_count).contains(&number) {
            return Some((
                SaveId::new(format!("autosave-{number}")),
                SaveKind::Autosave { generation: number },
                format!("Autosave {number}"),
            ));
        }
        return None;
    }
    let opaque = file_name
        .strip_prefix("manual-")?
        .strip_suffix(".factsim")?;
    if opaque.is_empty()
        || !opaque
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return None;
    }
    let id = SaveId::new(format!("manual-{opaque}"));
    Some((id, SaveKind::Named, format!("Named Save {opaque}")))
}

/// Performs the shared lightweight container/header classification used by
/// both catalog display and full recovery safety checks.
///
/// Reads that never observe file bytes (open and other I/O failures)
/// report `ValidationPending` instead of corruption so a transient denial
/// is retried rather than mislabelled; only observed malformation or
/// truncation reports `CorruptOrTruncated`.
pub(crate) fn inspect_file(path: &Path, kind: &SaveKind, current_hash: u64) -> FileInspection {
    match fs::File::open(path) {
        Ok(mut file) => inspect_open_file(&mut file, kind, current_hash),
        Err(_) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::ValidationPending,
            safe_to_replace: false,
            inspected: None,
        },
    }
}

/// Classifies the header visible through an already-open save handle and
/// records that handle's identity, binding the classification to the file
/// instance instead of the path.
fn inspect_open_file(file: &mut fs::File, kind: &SaveKind, current_hash: u64) -> FileInspection {
    let inspected = Some(save_file_metadata_fingerprint(file));
    let complete = |metadata, compatibility: SaveCompatibility| {
        let safe_to_replace = matches!(
            compatibility,
            SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave
        );
        FileInspection {
            metadata,
            compatibility,
            safe_to_replace,
            inspected: inspected.clone(),
        }
    };
    if file.rewind().is_err() {
        return FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::ValidationPending,
            safe_to_replace: false,
            inspected,
        };
    }
    match inspect_container_from_reader(file) {
        Ok(container) => {
            let compatibility = if container.version != CONTAINER_VERSION {
                SaveCompatibility::UnsupportedContainerVersion {
                    found: container.version,
                    supported: CONTAINER_VERSION,
                }
            } else {
                classify_inspection(&container.simulation_header, current_hash)
            };
            complete(container.metadata, compatibility)
        }
        Err(ContainerError::InvalidContainerMagic) if kind == &SaveKind::Quicksave => {
            let mut header = vec![0; SAVE_HEADER_SIZE];
            let read = file
                .rewind()
                .map_err(ContainerError::Io)
                .and_then(|()| read_inspection_bytes(file, &mut header));
            match read {
                Ok(()) => complete(None, classify_inspection(&header, current_hash)),
                // An established short file is corrupt; any other failure
                // observed no bytes and stays pending for a retry.
                Err(ContainerError::Truncated) => FileInspection {
                    metadata: None,
                    compatibility: SaveCompatibility::CorruptOrTruncated,
                    safe_to_replace: false,
                    inspected,
                },
                Err(_) => FileInspection {
                    metadata: None,
                    compatibility: SaveCompatibility::ValidationPending,
                    safe_to_replace: false,
                    inspected,
                },
            }
        }
        Err(ContainerError::InvalidContainerMagic) => {
            complete(None, SaveCompatibility::NotFactorySave)
        }
        Err(ContainerError::Io(_)) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::ValidationPending,
            safe_to_replace: false,
            inspected,
        },
        Err(_) => complete(None, SaveCompatibility::CorruptOrTruncated),
    }
}

/// Re-derives header compatibility from an already-open handle. Callers that
/// detected a path replacement use this so a stale classification is never
/// paired with a new file instance.
pub(crate) fn classify_open_save(
    file: &mut fs::File,
    kind: &SaveKind,
    current_hash: u64,
) -> SaveCompatibility {
    inspect_open_file(file, kind, current_hash).compatibility
}

/// Maps a simulation header parse into user-facing compatibility.
pub(crate) fn classify_inspection(header: &[u8], current_hash: u64) -> SaveCompatibility {
    match inspect_save_header(header) {
        Ok(header) => classify_header(header, current_hash),
        Err(factory_sim::SaveLoadError::InvalidMagic { .. }) => SaveCompatibility::NotFactorySave,
        Err(_) => SaveCompatibility::CorruptOrTruncated,
    }
}

/// Opens the file currently on disk and returns its identity together with the
/// header compatibility derived from that same handle. Retries queue this
/// pair so a classification is never bound to a different file instance.
pub(crate) fn inspect_current_file(
    path: &Path,
    kind: &SaveKind,
) -> Option<(SaveFileMetadataFingerprint, SaveCompatibility)> {
    let mut file = fs::File::open(path).ok()?;
    let metadata = save_file_metadata_fingerprint(&file);
    if metadata.len > factory_sim::SaveLimits::default().max_encoded_bytes {
        return Some((metadata, SaveCompatibility::ExceedsCurrentLimits));
    }
    let current_hash =
        factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().ok()?);
    let compatibility = classify_open_save(&mut file, kind, current_hash);
    Some((metadata, compatibility))
}

/// Returns a best-effort file modification timestamp for fallback metadata.
pub(crate) fn file_timestamp_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|timestamp| local_datetime_from_unix_ms(*timestamp).is_some())
        .unwrap_or(0)
}

/// Identifies the exact bytes considered by payload validation.
pub(crate) fn save_file_fingerprint(
    file: &mut (impl Read + Seek),
    metadata: SaveFileMetadataFingerprint,
) -> Result<SaveFileFingerprint, ContainerError> {
    file.rewind()?;
    let maximum = factory_sim::SaveLimits::default().max_encoded_bytes;
    if metadata.len > maximum {
        return Err(ContainerError::TooLarge);
    }
    let mut hasher = blake3::Hasher::new();
    let copied = std::io::copy(
        &mut Read::by_ref(file).take(maximum.saturating_add(1)),
        &mut hasher,
    )?;
    if copied > maximum {
        return Err(ContainerError::TooLarge);
    }
    Ok(SaveFileFingerprint {
        metadata: SaveFileMetadataFingerprint {
            len: copied,
            ..metadata
        },
        content_digest: *hasher.finalize().as_bytes(),
    })
}

pub(crate) fn save_file_metadata_fingerprint(file: &fs::File) -> SaveFileMetadataFingerprint {
    match file.metadata() {
        Ok(metadata) => SaveFileMetadataFingerprint {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            identity: save_file_identity(file, &metadata),
        },
        Err(_) => SaveFileMetadataFingerprint {
            len: 0,
            modified: None,
            identity: None,
        },
    }
}

#[cfg(unix)]
fn save_file_identity(_file: &fs::File, metadata: &fs::Metadata) -> Option<SaveFileIdentity> {
    use std::os::unix::fs::MetadataExt;

    let mut file_id = [0; 16];
    file_id[..8].copy_from_slice(&metadata.ino().to_le_bytes());
    Some(SaveFileIdentity {
        volume_or_device: metadata.dev(),
        file_id,
        change_time: metadata.ctime(),
        change_time_nanoseconds: metadata.ctime_nsec(),
    })
}

#[cfg(windows)]
fn save_file_identity(file: &fs::File, _metadata: &fs::Metadata) -> Option<SaveFileIdentity> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FILE_ID_INFO, FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx,
    };

    let mut id = FILE_ID_INFO::default();
    let mut basic = FILE_BASIC_INFO::default();
    // SAFETY: both output pointers refer to correctly sized writable structs,
    // and the borrowed file keeps the handle valid for both calls.
    let id_ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&raw mut id).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } != 0;
    // SAFETY: same argument validity as above, with FILE_BASIC_INFO.
    let basic_ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw mut basic).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } != 0;
    (id_ok && basic_ok).then_some(SaveFileIdentity {
        volume_or_device: id.VolumeSerialNumber,
        file_id: id.FileId.Identifier,
        change_time: basic.ChangeTime,
        change_time_nanoseconds: 0,
    })
}

#[cfg(not(any(unix, windows)))]
fn save_file_identity(_file: &fs::File, _metadata: &fs::Metadata) -> Option<SaveFileIdentity> {
    None
}

/// Returns the current wall-clock timestamp used in save metadata.
pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}
