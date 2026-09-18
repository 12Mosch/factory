//! Crash recovery: interrupted saves, stale artifacts, and backups.
//!
//! Best-effort recovery never prevents the ordinary catalog from listing
//! saves, and it never guesses between incompatible generations or
//! overwrites the only recoverable copy.

use super::super::SaveLoadConfig;
use super::super::container::{
    CONTAINER_VERSION, ContainerError, SaveArtifactKind, container_payload_offset,
    discard_save_artifact, inspect_container, load_simulation, parse_save_artifact, promote_backup,
    read_save_artifact, retired_save_artifact_primary,
};
use super::super::{SaveCompatibility, SaveId, SaveKind};
use super::inspect::{classify_inspection, inspect_file, recognized_file};
use bevy::log::warn;
use factory_sim::{SaveLoadError, load_from_bytes};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct RecoveryTarget {
    id: SaveId,
    kind: SaveKind,
    backups: Vec<PathBuf>,
    temporaries: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimaryState {
    Valid,
    IntactButIncompatible,
    Corrupt,
}

pub(crate) enum RecoveryBackup {
    Candidate(Vec<u8>),
    Corrupt,
    Inaccessible(ContainerError),
}

/// Best-effort recovery never prevents the ordinary catalog from listing saves.
pub(crate) fn recover_interrupted_saves(config: &SaveLoadConfig, current_hash: u64) {
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
        SaveCompatibility::Compatible | SaveCompatibility::MigratableSaveFormat { .. } => {
            match load_simulation(path) {
                Ok(_) => PrimaryState::Valid,
                Err(ContainerError::Simulation(error)) => classify_simulation_result(Err(error)),
                Err(ContainerError::Io(_) | ContainerError::TooLarge) => {
                    PrimaryState::IntactButIncompatible
                }
                Err(_) => PrimaryState::Corrupt,
            }
        }
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

pub(crate) fn classify_simulation_result(
    result: Result<factory_sim::Simulation, SaveLoadError>,
) -> PrimaryState {
    match result {
        Ok(_) => PrimaryState::Valid,
        Err(
            SaveLoadError::UnsupportedSaveVersion { .. }
            | SaveLoadError::UnsupportedPrototypeFormatVersion { .. }
            | SaveLoadError::TooLarge,
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
    let bytes = match read_save_artifact(path) {
        Ok(bytes) => bytes,
        // A bounded read cannot establish corruption. Retain oversized backups
        // too: they may be intact saves from a build with a different policy.
        Err(error) => return RecoveryBackup::Inaccessible(error),
    };

    if !bytes.starts_with(&super::super::container::CONTAINER_MAGIC) {
        if kind != &SaveKind::Quicksave {
            return RecoveryBackup::Corrupt;
        }
        return match classify_inspection(&bytes, current_hash) {
            SaveCompatibility::Compatible | SaveCompatibility::MigratableSaveFormat { .. } => {
                let result = load_from_bytes(&bytes);
                classify_backup_result(bytes, result)
            }
            SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
                RecoveryBackup::Corrupt
            }
            _ => RecoveryBackup::Candidate(bytes),
        };
    }

    let container = match inspect_container(path) {
        Ok(container) => container,
        Err(error @ ContainerError::Io(_)) => return RecoveryBackup::Inaccessible(error),
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
        SaveCompatibility::Compatible | SaveCompatibility::MigratableSaveFormat { .. } => {
            match container_payload_offset(&bytes) {
                Ok(offset) => {
                    let result = load_from_bytes(&bytes[offset..]);
                    classify_backup_result(bytes, result)
                }
                Err(error @ ContainerError::Io(_)) => RecoveryBackup::Inaccessible(error),
                Err(_) => RecoveryBackup::Corrupt,
            }
        }
        SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
            RecoveryBackup::Corrupt
        }
        _ => RecoveryBackup::Candidate(bytes),
    }
}

pub(crate) fn classify_backup_result(
    bytes: Vec<u8>,
    result: Result<factory_sim::Simulation, SaveLoadError>,
) -> RecoveryBackup {
    match result {
        Ok(_) => RecoveryBackup::Candidate(bytes),
        Err(SaveLoadError::TooLarge) => RecoveryBackup::Inaccessible(ContainerError::TooLarge),
        Err(_) => RecoveryBackup::Corrupt,
    }
}
