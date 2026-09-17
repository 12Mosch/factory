use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Parses a decimal world-seed display value back into its `u64`.
/// Returns `None` when the text is not a full decimal `u64`.
pub fn parse_world_seed(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok()
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
    SaveFormatOlder { found: u32, supported: u32 },
    SaveFormatNewer { found: u32, supported: u32 },
    PrototypeFormatOlder { found: u32, supported: u32 },
    PrototypeFormatNewer { found: u32, supported: u32 },
    PrototypeHashMismatch,
    UnsupportedContainerVersion { found: u32, supported: u32 },
    CorruptOrTruncated,
    NotFactorySave,
}

impl SaveCompatibility {
    pub fn can_load(&self) -> bool {
        matches!(self, Self::Compatible)
    }

    pub fn reason(&self) -> Option<String> {
        Some(match self {
            Self::Compatible => return None,
            Self::SaveFormatOlder { found, supported } => format!(
                "Save format {found} is older than supported format {supported}; this build has no migration for it."
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
            Self::CorruptOrTruncated => "The save file is incomplete or invalid.".into(),
            Self::NotFactorySave => "This file is not a Factory save.".into(),
        })
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Compatible => "Compatible",
            Self::SaveFormatOlder { .. } | Self::PrototypeFormatOlder { .. } => "Older format",
            Self::SaveFormatNewer { .. } | Self::PrototypeFormatNewer { .. } => "Newer format",
            Self::PrototypeHashMismatch => "Different data",
            Self::UnsupportedContainerVersion { .. } => "Unsupported container",
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
}

impl SaveEntry {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct SaveCatalog {
    entries: Vec<SaveEntry>,
    pub revision: u64,
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

    pub fn named_case_insensitive(&self, name: &str) -> Option<&SaveEntry> {
        let normalized = name.to_lowercase();
        self.entries.iter().find(|entry| {
            entry.metadata.kind == SaveKind::Named
                && entry.metadata.display_name.to_lowercase() == normalized
        })
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

    #[test]
    fn world_seed_display_round_trips_full_u64_range() {
        for seed in [0, 1, 123, 987654321, u64::MAX - 1, u64::MAX] {
            let text = format_world_seed(seed);
            assert_eq!(parse_world_seed(&text), Some(seed), "seed {seed}");
            assert_eq!(text, seed.to_string());
        }
    }

    #[test]
    fn world_seed_parse_rejects_non_decimal_u64() {
        assert_eq!(parse_world_seed(""), None);
        assert_eq!(parse_world_seed("  "), None);
        assert_eq!(parse_world_seed("abc"), None);
        assert_eq!(parse_world_seed("-1"), None);
        assert_eq!(parse_world_seed("18446744073709551616"), None);
        assert_eq!(parse_world_seed("12.5"), None);
        assert_eq!(parse_world_seed("  42  "), Some(42));
    }

    #[test]
    fn world_seed_label_distinguishes_known_and_legacy_saves() {
        let known = SaveMetadata {
            schema_version: 2,
            id: SaveId::new("test"),
            display_name: "Test".into(),
            kind: SaveKind::Named,
            completed_at_unix_ms: 0,
            application_version: "test".into(),
            world_seed: Some(u64::MAX),
        };
        assert_eq!(known.world_seed_label(), format!("Seed {}", u64::MAX));
        assert_eq!(
            parse_world_seed(known.world_seed_label().trim_start_matches("Seed ")),
            Some(u64::MAX)
        );

        let legacy = SaveMetadata {
            world_seed: None,
            ..known.clone()
        };
        assert_eq!(legacy.world_seed_label(), "Seed unknown");
    }

    #[test]
    fn legacy_metadata_without_seed_decodes_to_none() {
        let legacy = r#"(
            schema_version: 1,
            id: "manual-test",
            display_name: "Legacy",
            kind: Named,
            completed_at_unix_ms: 42,
            application_version: "0.1.0",
        )"#;
        let decoded: SaveMetadata = ron::de::from_str(legacy).expect("v1 metadata decodes");
        assert_eq!(decoded.world_seed, None);
        assert_eq!(decoded.schema_version, 1);
    }

    #[test]
    fn current_metadata_with_max_seed_round_trips() {
        let metadata = SaveMetadata {
            schema_version: 2,
            id: SaveId::new("manual-test"),
            display_name: "Max".into(),
            kind: SaveKind::Named,
            completed_at_unix_ms: 42,
            application_version: "test".into(),
            world_seed: Some(u64::MAX),
        };
        let text = ron::ser::to_string(&metadata).expect("metadata encodes");
        let decoded: SaveMetadata = ron::de::from_str(&text).expect("metadata decodes");
        assert_eq!(decoded, metadata);
        assert_eq!(
            parse_world_seed(&format_world_seed(u64::MAX)),
            Some(u64::MAX)
        );
    }
}
