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
