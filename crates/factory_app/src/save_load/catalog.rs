use super::compatibility::classify_header;
use super::container::{
    CONTAINER_VERSION, ContainerError, SaveArtifactKind, discard_save_artifact, fallback_metadata,
    inspect_container, parse_save_artifact, promote_backup, read_simulation_payload,
    retired_save_artifact_primary, with_save_artifact_lock,
};
use super::{
    SaveCatalog, SaveCompatibility, SaveEntry, SaveId, SaveKind, SaveLoadConfig, SaveMetadata,
    local_datetime_from_unix_ms,
};
use bevy::log::warn;
use factory_data::PrototypeCatalog;
use factory_sim::{
    SAVE_HEADER_SIZE, SaveLoadError, inspect_save_header, load_from_bytes, prototype_hash,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Replaces the in-memory catalog with a freshly recovered and inspected scan.
pub fn refresh_catalog(config: &SaveLoadConfig, catalog: &mut SaveCatalog) -> Result<(), String> {
    let entries = scan_catalog(config)?;
    catalog.replace(entries);
    Ok(())
}

/// Recovers interrupted saves and returns all recognized canonical entries.
pub fn scan_catalog(config: &SaveLoadConfig) -> Result<Vec<SaveEntry>, String> {
    if !config.root_dir.exists() {
        return Ok(Vec::new());
    }
    let current_hash = prototype_hash(
        &PrototypeCatalog::load_base()
            .map_err(|error| format!("failed to load prototype data: {error}"))?,
    );
    with_save_artifact_lock(|| recover_interrupted_saves(config, current_hash));
    let directory = fs::read_dir(&config.root_dir)
        .map_err(|error| format!("failed to scan save directory: {error}"))?;
    let mut entries = Vec::new();
    for item in directory {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                warn!("failed to inspect a save-directory entry: {error}");
                continue;
            }
        };
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some((id, kind, fallback_name)) = recognized_file(&path, config.autosave_slot_count)
        else {
            continue;
        };
        entries.push(inspect_entry(path, id, kind, fallback_name, current_hash));
    }
    entries.sort_by(|left, right| {
        group_order(&left.metadata.kind)
            .cmp(&group_order(&right.metadata.kind))
            .then_with(|| {
                right
                    .metadata
                    .completed_at_unix_ms
                    .cmp(&left.metadata.completed_at_unix_ms)
            })
            .then_with(|| {
                autosave_generation(&left.metadata.kind)
                    .cmp(&autosave_generation(&right.metadata.kind))
            })
    });
    Ok(entries)
}

#[derive(Debug)]
struct RecoveryTarget {
    id: SaveId,
    kind: SaveKind,
    backups: Vec<PathBuf>,
    temporaries: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryState {
    Valid,
    IntactButIncompatible,
    Corrupt,
}

enum RecoveryBackup {
    Candidate(Vec<u8>),
    Corrupt,
    Inaccessible(io::Error),
}

struct FileInspection {
    metadata: Option<SaveMetadata>,
    compatibility: SaveCompatibility,
    safe_to_replace: bool,
}

/// Best-effort recovery never prevents the ordinary catalog from listing saves.
fn recover_interrupted_saves(config: &SaveLoadConfig, current_hash: u64) {
    let directory = match fs::read_dir(&config.root_dir) {
        Ok(directory) => directory,
        Err(error) => {
            warn!("failed to scan save recovery files: {error}");
            return;
        }
    };
    let mut targets = BTreeMap::<PathBuf, RecoveryTarget>::new();
    for item in directory {
        let item = match item {
            Ok(item) => item,
            Err(error) => {
                warn!("failed to inspect a save recovery entry: {error}");
                continue;
            }
        };
        let artifact = item.path();
        if !artifact.is_file() {
            continue;
        }
        if retired_save_artifact_primary(&artifact).is_some() {
            remove_recovery_artifact(&artifact);
            continue;
        }
        let Some((primary, artifact_kind)) = parse_save_artifact(&artifact) else {
            continue;
        };
        let Some((id, kind, _)) = recognized_file(&primary, config.autosave_slot_count) else {
            continue;
        };
        let target = targets.entry(primary).or_insert_with(|| RecoveryTarget {
            id,
            kind,
            backups: Vec::new(),
            temporaries: Vec::new(),
        });
        match artifact_kind {
            SaveArtifactKind::Temporary => target.temporaries.push(artifact),
            SaveArtifactKind::Backup => target.backups.push(artifact),
        }
    }

    for (primary, target) in targets {
        for temporary in target.temporaries {
            remove_recovery_artifact(&temporary);
        }
        let primary_exists = match primary.try_exists() {
            Ok(exists) => exists,
            Err(error) => {
                warn!(
                    "cannot inspect save {} for recovery: {error}",
                    primary.display()
                );
                continue;
            }
        };
        let state = primary_exists.then(|| primary_state(&primary, &target.kind, current_hash));
        match state {
            Some(PrimaryState::Valid) => {
                for backup in target.backups {
                    remove_recovery_artifact(&backup);
                }
                continue;
            }
            Some(PrimaryState::IntactButIncompatible) => continue,
            Some(PrimaryState::Corrupt) | None => {}
        }

        let mut candidates: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        let mut validation_deferred = false;
        for backup in target.backups {
            match validate_recovery_backup(&backup, &target.id, &target.kind, current_hash) {
                RecoveryBackup::Candidate(bytes) => {
                    if candidates.iter().any(|(_, existing)| existing == &bytes) {
                        remove_recovery_artifact(&backup);
                    } else {
                        candidates.push((backup, bytes));
                    }
                }
                RecoveryBackup::Corrupt => remove_recovery_artifact(&backup),
                RecoveryBackup::Inaccessible(error) => {
                    validation_deferred = true;
                    warn!(
                        "cannot validate recovery backup {}: {error}",
                        backup.display()
                    );
                }
            }
        }
        if validation_deferred || candidates.len() != 1 {
            continue;
        }
        let (backup, _) = candidates.pop().expect("length checked");
        if let Err(error) = promote_backup(&backup, &primary, primary_exists) {
            warn!(
                "failed to recover save {} from {}: {error}",
                primary.display(),
                backup.display()
            );
        }
    }
}

/// Removes one stale artifact without aborting catalog availability on failure.
fn remove_recovery_artifact(path: &Path) {
    if let Err(error) = discard_save_artifact(path) {
        warn!(
            "failed to remove stale save artifact {}: {error}",
            path.display()
        );
    }
}

/// Fully classifies a primary only when lightweight inspection says it is compatible.
fn primary_state(path: &Path, kind: &SaveKind, current_hash: u64) -> PrimaryState {
    let inspection = inspect_file(path, kind, current_hash);
    match inspection.compatibility {
        SaveCompatibility::Compatible => match read_simulation_payload(path) {
            Ok(payload) => simulation_payload_state(&payload),
            Err(ContainerError::Io(_)) => PrimaryState::IntactButIncompatible,
            Err(_) => PrimaryState::Corrupt,
        },
        SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
            if inspection.safe_to_replace {
                PrimaryState::Corrupt
            } else {
                PrimaryState::IntactButIncompatible
            }
        }
        _ => PrimaryState::IntactButIncompatible,
    }
}

/// Distinguishes corruption from an intact payload requiring another game version.
fn simulation_payload_state(payload: &[u8]) -> PrimaryState {
    match load_from_bytes(payload) {
        Ok(_) => PrimaryState::Valid,
        Err(
            SaveLoadError::UnsupportedSaveVersion { .. }
            | SaveLoadError::UnsupportedPrototypeFormatVersion { .. },
        ) => PrimaryState::IntactButIncompatible,
        Err(
            SaveLoadError::InvalidMagic { .. }
            | SaveLoadError::PrototypeHashMismatch { .. }
            | SaveLoadError::InvalidSimulationState(_)
            | SaveLoadError::Codec(_),
        ) => PrimaryState::Corrupt,
    }
}

/// Classifies a backup without deleting data that is merely incompatible or
/// temporarily unreadable. Candidate bytes are retained for duplicate checks.
fn validate_recovery_backup(
    path: &Path,
    id: &SaveId,
    kind: &SaveKind,
    current_hash: u64,
) -> RecoveryBackup {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return RecoveryBackup::Inaccessible(error),
    };

    if !bytes.starts_with(&super::container::CONTAINER_MAGIC) {
        if kind != &SaveKind::Quicksave {
            return RecoveryBackup::Corrupt;
        }
        return match classify_inspection(&bytes, current_hash) {
            SaveCompatibility::Compatible => match load_from_bytes(&bytes) {
                Ok(_) => RecoveryBackup::Candidate(bytes),
                Err(_) => RecoveryBackup::Corrupt,
            },
            SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
                RecoveryBackup::Corrupt
            }
            _ => RecoveryBackup::Candidate(bytes),
        };
    }

    let container = match inspect_container(path) {
        Ok(container) => container,
        Err(ContainerError::Io(error)) => return RecoveryBackup::Inaccessible(error),
        Err(_) => return RecoveryBackup::Corrupt,
    };
    if container
        .metadata
        .as_ref()
        .is_some_and(|metadata| &metadata.id != id || &metadata.kind != kind)
    {
        return RecoveryBackup::Corrupt;
    }
    if container.version != CONTAINER_VERSION {
        return RecoveryBackup::Candidate(bytes);
    }

    match classify_inspection(&container.simulation_header, current_hash) {
        SaveCompatibility::Compatible => match read_simulation_payload(path) {
            Ok(payload) if load_from_bytes(&payload).is_ok() => RecoveryBackup::Candidate(bytes),
            Ok(_) => RecoveryBackup::Corrupt,
            Err(ContainerError::Io(error)) => RecoveryBackup::Inaccessible(error),
            Err(_) => RecoveryBackup::Corrupt,
        },
        SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
            RecoveryBackup::Corrupt
        }
        _ => RecoveryBackup::Candidate(bytes),
    }
}

/// Maps canonical file names to stable save identities and kinds.
fn recognized_file(path: &Path, autosave_count: usize) -> Option<(SaveId, SaveKind, String)> {
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

/// Builds one catalog entry using the shared lightweight file inspection.
fn inspect_entry(
    path: PathBuf,
    id: SaveId,
    kind: SaveKind,
    fallback_name: String,
    current_hash: u64,
) -> SaveEntry {
    let timestamp = file_timestamp_ms(&path);
    let fallback = || fallback_metadata(id.clone(), kind.clone(), fallback_name.clone(), timestamp);
    let inspection = inspect_file(&path, &kind, current_hash);
    let metadata = inspection
        .metadata
        .filter(|metadata| metadata.id == id && metadata.kind == kind);
    let metadata_available = metadata.as_ref().is_some_and(|metadata| {
        local_datetime_from_unix_ms(metadata.completed_at_unix_ms).is_some()
    });
    let metadata = metadata
        .map(|mut metadata| {
            if !metadata_available {
                metadata.completed_at_unix_ms = timestamp;
            }
            metadata
        })
        .unwrap_or_else(fallback);
    SaveEntry {
        id,
        metadata,
        compatibility: inspection.compatibility,
        metadata_available,
        path,
    }
}

/// Performs the shared lightweight container/header classification used by
/// both catalog display and full recovery safety checks.
fn inspect_file(path: &Path, kind: &SaveKind, current_hash: u64) -> FileInspection {
    match inspect_container(path) {
        Ok(container) => {
            let compatibility = if container.version != CONTAINER_VERSION {
                SaveCompatibility::UnsupportedContainerVersion {
                    found: container.version,
                    supported: CONTAINER_VERSION,
                }
            } else {
                classify_inspection(&container.simulation_header, current_hash)
            };
            FileInspection {
                metadata: container.metadata,
                safe_to_replace: matches!(
                    compatibility,
                    SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave
                ),
                compatibility,
            }
        }
        Err(ContainerError::InvalidContainerMagic) if kind == &SaveKind::Quicksave => {
            let mut header = vec![0; SAVE_HEADER_SIZE];
            let (compatibility, safe_to_replace) = match fs::File::open(path)
                .and_then(|mut file| file.read_exact(&mut header))
            {
                Ok(()) => {
                    let compatibility = classify_inspection(&header, current_hash);
                    let safe_to_replace = matches!(
                        compatibility,
                        SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave
                    );
                    (compatibility, safe_to_replace)
                }
                Err(_) => (SaveCompatibility::CorruptOrTruncated, false),
            };
            FileInspection {
                metadata: None,
                safe_to_replace,
                compatibility,
            }
        }
        Err(ContainerError::InvalidContainerMagic) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::NotFactorySave,
            safe_to_replace: true,
        },
        Err(ContainerError::Io(_)) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::CorruptOrTruncated,
            safe_to_replace: false,
        },
        Err(_) => FileInspection {
            metadata: None,
            compatibility: SaveCompatibility::CorruptOrTruncated,
            safe_to_replace: true,
        },
    }
}

/// Maps a simulation header parse into user-facing compatibility.
fn classify_inspection(header: &[u8], current_hash: u64) -> SaveCompatibility {
    match inspect_save_header(header) {
        Ok(header) => classify_header(header, current_hash),
        Err(SaveLoadError::InvalidMagic { .. }) => SaveCompatibility::NotFactorySave,
        Err(_) => SaveCompatibility::CorruptOrTruncated,
    }
}

/// Returns a best-effort file modification timestamp for fallback metadata.
fn file_timestamp_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .filter(|timestamp| local_datetime_from_unix_ms(*timestamp).is_some())
        .unwrap_or(0)
}

/// Assigns stable catalog groups for sorting.
fn group_order(kind: &SaveKind) -> u8 {
    match kind {
        SaveKind::Named => 0,
        SaveKind::Quicksave => 1,
        SaveKind::Autosave { .. } => 2,
    }
}

/// Extracts an autosave generation for deterministic tie-breaking.
fn autosave_generation(kind: &SaveKind) -> usize {
    match kind {
        SaveKind::Autosave { generation } => *generation,
        _ => 0,
    }
}

/// Returns the current wall-clock timestamp used in save metadata.
pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_and_unrelated_file_names_are_ignored() {
        let count = 5;
        for name in [
            "slot_1.factsim",
            "slot_2.factsim",
            "slot_3.factsim",
            "autosave.factsim",
            "quicksave.factsim.tmp-1",
            "file.txt",
        ] {
            assert!(recognized_file(Path::new(name), count).is_none());
        }
        assert!(recognized_file(Path::new("manual-abc.factsim"), count).is_some());
        assert!(recognized_file(Path::new("autosave-5.factsim"), count).is_some());
    }
}
