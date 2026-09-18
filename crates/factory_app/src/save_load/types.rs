use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SaveId(String);

impl SaveId {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveKind {
    Named,
    Quicksave,
    Autosave { generation: usize },
}

impl SaveKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Named => "Named",
            Self::Quicksave => "Quicksave",
            Self::Autosave { .. } => "Autosave",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveMetadata {
    pub schema_version: u32,
    pub id: SaveId,
    pub display_name: String,
    pub kind: SaveKind,
    pub completed_at_unix_ms: u64,
    pub application_version: String,
    /// World seed preserved without deserializing the simulation payload.
    /// `None` for saves written before the seed was recorded in metadata.
    #[serde(default)]
    pub world_seed: Option<u64>,
}

/// Formats a world seed as its full decimal `u64` representation.
pub fn format_world_seed(seed: u64) -> String {
    seed.to_string()
}

impl SaveMetadata {
    /// Human-readable seed fragment for catalog rows.
    pub fn world_seed_label(&self) -> String {
        match self.world_seed {
            Some(seed) => format!("Seed {}", format_world_seed(seed)),
            None => "Seed unknown".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveCompatibility {
    Compatible,
    MigratableSaveFormat { found: u32, current: u32 },
    ValidationPending,
    SaveFormatOlder { found: u32, supported: u32 },
    SaveFormatNewer { found: u32, supported: u32 },
    PrototypeFormatOlder { found: u32, supported: u32 },
    PrototypeFormatNewer { found: u32, supported: u32 },
    PrototypeHashMismatch,
    UnsupportedContainerVersion { found: u32, supported: u32 },
    ExceedsCurrentLimits,
    CorruptOrTruncated,
    NotFactorySave,
}

impl SaveCompatibility {
    pub fn can_load(&self) -> bool {
        matches!(self, Self::Compatible | Self::MigratableSaveFormat { .. })
    }

    pub fn reason(&self) -> Option<String> {
        Some(match self {
            Self::Compatible => return None,
            Self::MigratableSaveFormat { found, current } => format!(
                "Save format {found} will be migrated to {current} when loaded. The source file remains unchanged until you explicitly save."
            ),
            Self::ValidationPending => {
                "The save payload is being checked before loading is enabled.".into()
            }
            Self::SaveFormatOlder { found, supported } => format!(
                "Save format {found} predates the oldest supported migration source ({supported}). Open it with a build that supports that format, then re-save it before updating."
            ),
            Self::SaveFormatNewer { found, supported } => format!(
                "Save format {found} was created by a newer build (this build supports {supported}); update the game to load it."
            ),
            Self::PrototypeFormatOlder { found, supported } => format!(
                "Prototype format {found} is older than supported format {supported}; this build has no migration for it."
            ),
            Self::PrototypeFormatNewer { found, supported } => format!(
                "Prototype format {found} was created by a newer build (this build supports {supported}); update the game to load it."
            ),
            Self::PrototypeHashMismatch => "This save uses different game/prototype data and may come from another build or data set.".into(),
            Self::UnsupportedContainerVersion { found, supported } => format!(
                "Container version {found} is unsupported; this build supports version {supported}."
            ),
            Self::ExceedsCurrentLimits => {
                "The save exceeds this build's size or collection limits.".into()
            }
            Self::CorruptOrTruncated => "The save file is incomplete or invalid.".into(),
            Self::NotFactorySave => "This file is not a Factory save.".into(),
        })
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Compatible => "Compatible",
            Self::MigratableSaveFormat { .. } => "Migratable",
            Self::ValidationPending => "Checking...",
            Self::SaveFormatOlder { .. } => "Unsupported old format",
            Self::PrototypeFormatOlder { .. } => "Older prototype format",
            Self::SaveFormatNewer { .. } | Self::PrototypeFormatNewer { .. } => "Newer format",
            Self::PrototypeHashMismatch => "Different data",
            Self::UnsupportedContainerVersion { .. } => "Unsupported container",
            Self::ExceedsCurrentLimits => "Exceeds limits",
            Self::CorruptOrTruncated => "Invalid file",
            Self::NotFactorySave => "Not a Factory save",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveEntry {
    pub id: SaveId,
    pub metadata: SaveMetadata,
    pub compatibility: SaveCompatibility,
    pub metadata_available: bool,
    pub(crate) path: PathBuf,
    /// Identity of the file instance whose bytes produced `compatibility`.
    /// Validation must re-derive the classification when the path no longer
    /// resolves to this instance.
    pub(crate) inspected: Option<SaveFileMetadataFingerprint>,
}

impl SaveEntry {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[derive(Resource, Debug, Default)]
pub struct SaveCatalog {
    pub(crate) entries: Vec<SaveEntry>,
    pub revision: u64,
    pub(crate) validation_cache: BTreeMap<PathBuf, CachedSaveValidation>,
    pub(crate) validation_queue: VecDeque<CatalogValidationRequest>,
    pub(crate) validation_jobs: Vec<CatalogValidationJob>,
    /// Earliest time at which entries stuck at `ValidationPending` with no
    /// scheduled validation are re-observed. Monotonic so a backward wall-
    /// clock jump cannot suspend rescans; `None` means a rescan is due.
    pub(crate) next_pending_rescan: Option<Instant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SaveFileIdentity {
    pub(crate) volume_or_device: u64,
    pub(crate) file_id: [u8; 16],
    pub(crate) change_time: i64,
    pub(crate) change_time_nanoseconds: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SaveFileMetadataFingerprint {
    pub(crate) len: u64,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) identity: Option<SaveFileIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SaveFileFingerprint {
    pub(crate) metadata: SaveFileMetadataFingerprint,
    pub(crate) content_digest: [u8; 32],
}

#[derive(Clone, Debug)]
pub(crate) struct CachedSaveValidation {
    pub(crate) fingerprint: SaveFileFingerprint,
    pub(crate) compatibility: SaveCompatibility,
}

#[derive(Clone, Debug)]
pub(crate) struct CatalogValidationRequest {
    pub(crate) path: PathBuf,
    pub(crate) kind: SaveKind,
    pub(crate) compatibility: SaveCompatibility,
    pub(crate) metadata: SaveFileMetadataFingerprint,
    pub(crate) attempt: u8,
}

#[derive(Debug)]
pub(crate) struct CatalogValidationOutcome {
    pub(crate) path: PathBuf,
    pub(crate) compatibility: SaveCompatibility,
    pub(crate) observed_metadata: Option<SaveFileMetadataFingerprint>,
    pub(crate) fingerprint: Option<SaveFileFingerprint>,
    pub(crate) attempt: u8,
}

#[derive(Debug)]
pub(crate) struct CatalogValidationJob {
    pub(crate) path: PathBuf,
    pub(crate) metadata: SaveFileMetadataFingerprint,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) handle: JoinHandle<CatalogValidationOutcome>,
}

impl SaveCatalog {
    pub fn entries(&self) -> &[SaveEntry] {
        &self.entries
    }

    pub fn get(&self, id: &SaveId) -> Option<&SaveEntry> {
        self.entries.iter().find(|entry| &entry.id == id)
    }

    pub(crate) fn replace(&mut self, entries: Vec<SaveEntry>) {
        self.entries = entries;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn invalidate_validation(&mut self, id: &SaveId) {
        if let Some(path) = self.get(id).map(|entry| entry.path.clone()) {
            self.validation_cache.remove(&path);
            self.validation_queue.retain(|request| request.path != path);
        }
    }

    pub fn named_case_insensitive(&self, name: &str) -> Option<&SaveEntry> {
        let normalized = name.to_lowercase();
        self.entries.iter().find(|entry| {
            entry.metadata.kind == SaveKind::Named
                && entry.metadata.display_name.to_lowercase() == normalized
        })
    }
}

impl Drop for SaveCatalog {
    fn drop(&mut self) {
        // Signal cancellation first: workers abort at their next read chunk
        // instead of hashing and decoding the remainder, so shutdown does
        // not wait for large saves. Joining afterwards stays deterministic:
        // no detached worker can outlive the catalog and hold save files.
        for job in &self.validation_jobs {
            job.cancel.store(true, Ordering::Relaxed);
        }
        for job in self.validation_jobs.drain(..) {
            let _ = job.handle.join();
        }
    }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub enum PendingSaveConfirmation {
    #[default]
    None,
    Overwrite(SaveId),
    Delete(SaveId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SaveLoadTab {
    #[default]
    Save,
    Load,
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadWindowState {
    pub open: bool,
    pub tab: SaveLoadTab,
    pub name_buffer: String,
    pub refresh_on_open: bool,
}

impl Default for SaveLoadWindowState {
    fn default() -> Self {
        Self {
            open: false,
            tab: SaveLoadTab::Save,
            name_buffer: String::new(),
            refresh_on_open: false,
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SaveLoadStatus {
    pub message: Option<String>,
    pub kind: SaveLoadStatusKind,
    pub last_completed_id: Option<SaveId>,
}

impl Default for SaveLoadStatus {
    fn default() -> Self {
        Self {
            message: None,
            kind: SaveLoadStatusKind::Info,
            last_completed_id: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SaveLoadStatusKind {
    #[default]
    Info,
    Success,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata(world_seed: Option<u64>) -> SaveMetadata {
        SaveMetadata {
            schema_version: 2,
            id: SaveId::new("test"),
            display_name: "Test".into(),
            kind: SaveKind::Named,
            completed_at_unix_ms: 0,
            application_version: "test".into(),
            world_seed,
        }
    }

    #[test]
    fn world_seed_format_and_label_round_trip_full_u64() {
        for seed in [0, 123, u64::MAX] {
            let text = format_world_seed(seed);
            assert_eq!(text, seed.to_string());
            assert_eq!(text.parse::<u64>().ok(), Some(seed), "seed {seed}");
            let metadata = test_metadata(Some(seed));
            assert_eq!(metadata.world_seed_label(), format!("Seed {seed}"));
            assert_eq!(
                metadata
                    .world_seed_label()
                    .trim_start_matches("Seed ")
                    .parse::<u64>()
                    .ok(),
                Some(seed)
            );
        }
        assert_eq!(test_metadata(None).world_seed_label(), "Seed unknown");
    }
}
