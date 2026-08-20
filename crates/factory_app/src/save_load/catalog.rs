use super::compatibility::classify_header;
use super::container::{
    CONTAINER_VERSION, ContainerError, METADATA_SCHEMA_VERSION, decode_container,
    fallback_metadata, inspect_container, promote_backup, read_simulation_payload,
};
use super::{SaveCatalog, SaveCompatibility, SaveEntry, SaveId, SaveKind, SaveLoadConfig};
use factory_data::PrototypeCatalog;
use factory_sim::{
    SAVE_HEADER_SIZE, SaveLoadError, inspect_save_header, load_from_bytes, prototype_hash,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn refresh_catalog(config: &SaveLoadConfig, catalog: &mut SaveCatalog) -> Result<(), String> {
    let entries = scan_catalog(config)?;
    catalog.replace(entries);
    Ok(())
}

pub fn scan_catalog(config: &SaveLoadConfig) -> Result<Vec<SaveEntry>, String> {
    if !config.root_dir.exists() {
        return Ok(Vec::new());
    }
    let current_hash = prototype_hash(
        &PrototypeCatalog::load_base()
            .map_err(|error| format!("failed to load prototype data: {error}"))?,
    );
    recover_interrupted_saves(config, current_hash)?;
    let directory = fs::read_dir(&config.root_dir)
        .map_err(|error| format!("failed to scan save directory: {error}"))?;
    let mut entries = Vec::new();
    for item in directory {
        let item = item.map_err(|error| format!("failed to scan save directory: {error}"))?;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryState {
    Valid,
    IntactButIncompatible,
    Corrupt,
}

fn recover_interrupted_saves(config: &SaveLoadConfig, current_hash: u64) -> Result<(), String> {
    let directory = fs::read_dir(&config.root_dir)
        .map_err(|error| format!("failed to scan save recovery files: {error}"))?;
    let mut targets = BTreeMap::<PathBuf, RecoveryTarget>::new();
    for item in directory {
        let item = item.map_err(|error| format!("failed to scan save recovery files: {error}"))?;
        let backup = item.path();
        if !backup.is_file() {
            continue;
        }
        let Some((primary, id, kind)) = recognized_backup(&backup, config.autosave_slot_count)
        else {
            continue;
        };
        targets
            .entry(primary)
            .or_insert_with(|| RecoveryTarget {
                id,
                kind,
                backups: Vec::new(),
            })
            .backups
            .push(backup);
    }

    for (primary, target) in targets {
        let primary_needs_recovery = if primary.exists() {
            primary_state(&primary, &target.kind, current_hash) == PrimaryState::Corrupt
        } else {
            true
        };
        if !primary_needs_recovery {
            continue;
        }
        let mut valid_backups = target.backups.into_iter().filter(|backup| {
            is_valid_recovery_backup(backup, &target.id, &target.kind, current_hash)
        });
        let Some(backup) = valid_backups.next() else {
            continue;
        };
        if valid_backups.next().is_some() {
            continue;
        }
        promote_backup(&backup, &primary).map_err(|error| {
            format!(
                "failed to recover save {} from {}: {error}",
                primary.display(),
                backup.display()
            )
        })?;
    }
    Ok(())
}

fn recognized_backup(path: &Path, autosave_count: usize) -> Option<(PathBuf, SaveId, SaveKind)> {
    let file_name = path.file_name()?.to_str()?;
    let (base, suffix) = file_name.rsplit_once(".factsim.bak-")?;
    if suffix.is_empty()
        || !suffix
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return None;
    }
    let primary = path.with_file_name(format!("{base}.factsim"));
    let (id, kind, _) = recognized_file(&primary, autosave_count)?;
    Some((primary, id, kind))
}

fn primary_state(path: &Path, kind: &SaveKind, current_hash: u64) -> PrimaryState {
    match inspect_container(path) {
        Ok(container) if container.version != CONTAINER_VERSION => {
            PrimaryState::IntactButIncompatible
        }
        Ok(container) => match classify_inspection(&container.simulation_header, current_hash) {
            SaveCompatibility::Compatible => match read_simulation_payload(path) {
                Ok(payload) => simulation_payload_state(&payload),
                Err(ContainerError::Io(_)) => PrimaryState::IntactButIncompatible,
                Err(_) => PrimaryState::Corrupt,
            },
            SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
                PrimaryState::Corrupt
            }
            _ => PrimaryState::IntactButIncompatible,
        },
        Err(ContainerError::InvalidContainerMagic) if kind == &SaveKind::Quicksave => {
            match fs::read(path) {
                Ok(payload) => match classify_inspection(&payload, current_hash) {
                    SaveCompatibility::Compatible => simulation_payload_state(&payload),
                    SaveCompatibility::CorruptOrTruncated | SaveCompatibility::NotFactorySave => {
                        PrimaryState::Corrupt
                    }
                    _ => PrimaryState::IntactButIncompatible,
                },
                Err(_) => PrimaryState::IntactButIncompatible,
            }
        }
        Err(ContainerError::Io(_)) => PrimaryState::IntactButIncompatible,
        Err(_) => PrimaryState::Corrupt,
    }
}

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

fn is_valid_recovery_backup(path: &Path, id: &SaveId, kind: &SaveKind, current_hash: u64) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(container) = inspect_container(path) else {
        return false;
    };
    if container.version != CONTAINER_VERSION {
        return false;
    }
    let Ok((metadata, payload)) = decode_container(&bytes) else {
        return false;
    };
    metadata.schema_version == METADATA_SCHEMA_VERSION
        && &metadata.id == id
        && &metadata.kind == kind
        && classify_inspection(payload, current_hash) == SaveCompatibility::Compatible
        && load_from_bytes(payload).is_ok()
}

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

fn inspect_entry(
    path: PathBuf,
    id: SaveId,
    kind: SaveKind,
    fallback_name: String,
    current_hash: u64,
) -> SaveEntry {
    let timestamp = file_timestamp_ms(&path);
    let fallback = || fallback_metadata(id.clone(), kind.clone(), fallback_name.clone(), timestamp);
    let mut metadata_available = false;
    let (metadata, compatibility) = match inspect_container(&path) {
        Ok(container) => {
            let metadata = container
                .metadata
                .filter(|metadata| metadata.id == id && metadata.kind == kind);
            metadata_available = metadata.is_some();
            let compatibility = if container.version != CONTAINER_VERSION {
                SaveCompatibility::UnsupportedContainerVersion {
                    found: container.version,
                    supported: CONTAINER_VERSION,
                }
            } else {
                classify_inspection(&container.simulation_header, current_hash)
            };
            (metadata.unwrap_or_else(fallback), compatibility)
        }
        Err(ContainerError::InvalidContainerMagic) if kind == SaveKind::Quicksave => {
            let mut header = vec![0; SAVE_HEADER_SIZE];
            let compatibility =
                match fs::File::open(&path).and_then(|mut file| file.read_exact(&mut header)) {
                    Ok(()) => classify_inspection(&header, current_hash),
                    Err(_) => SaveCompatibility::CorruptOrTruncated,
                };
            (fallback(), compatibility)
        }
        Err(ContainerError::InvalidContainerMagic) => {
            (fallback(), SaveCompatibility::NotFactorySave)
        }
        Err(_) => (fallback(), SaveCompatibility::CorruptOrTruncated),
    };
    SaveEntry {
        id,
        metadata,
        compatibility,
        metadata_available,
        path,
    }
}

fn classify_inspection(header: &[u8], current_hash: u64) -> SaveCompatibility {
    match inspect_save_header(header) {
        Ok(header) => classify_header(header, current_hash),
        Err(SaveLoadError::InvalidMagic { .. }) => SaveCompatibility::NotFactorySave,
        Err(_) => SaveCompatibility::CorruptOrTruncated,
    }
}

fn file_timestamp_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis() as u64)
}

fn group_order(kind: &SaveKind) -> u8 {
    match kind {
        SaveKind::Named => 0,
        SaveKind::Quicksave => 1,
        SaveKind::Autosave { .. } => 2,
    }
}

fn autosave_generation(kind: &SaveKind) -> usize {
    match kind {
        SaveKind::Autosave { generation } => *generation,
        _ => 0,
    }
}

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
