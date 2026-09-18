//! Format safety limits, independent of gameplay and supported-world budgets.
//!
//! The current format is uncompressed: decoded bytes means the uncompressed
//! wire representation, not Rust heap usage. Collection limits are checked
//! before serde can reserve storage. Future compressed/record formats must
//! enforce their decoded and record budgets before allocating/decompressing.

/// Shared policy for both sides of the durable save boundary.
#[derive(Clone, Copy, Debug)]
pub struct SaveLimits {
    /// Complete artifact, including container metadata and simulation header.
    pub max_encoded_bytes: u64,
    /// Uncompressed snapshot wire bytes, excluding headers and metadata.
    pub max_decoded_bytes: u64,
    /// Maximum entries in any variable-length collection (bytes for strings).
    pub max_collection_entries: u64,
    /// Maximum bytes for one record. The monolithic snapshot counts as a
    /// single record; the indexed record container enforces this per record
    /// and has no compression.
    pub max_record_bytes: u64,
    pub max_metadata_bytes: usize,
}

pub const SAVE_METADATA_BYTES: usize = 16 * 1024;
pub const SAVE_CONTAINER_PREFIX_BYTES: usize = 16;

impl Default for SaveLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: crate::MAX_SNAPSHOT_BYTES
                + crate::SAVE_HEADER_SIZE as u64
                + SAVE_CONTAINER_PREFIX_BYTES as u64
                + SAVE_METADATA_BYTES as u64,
            max_decoded_bytes: crate::MAX_SNAPSHOT_BYTES,
            // No collection in the current schema has zero-byte entries.
            // Thus this preserves the existing 64 MiB wire-format budget.
            max_collection_entries: crate::MAX_SNAPSHOT_BYTES,
            max_record_bytes: crate::MAX_SNAPSHOT_BYTES,
            max_metadata_bytes: SAVE_METADATA_BYTES,
        }
    }
}

impl SaveLimits {
    pub fn max_simulation_bytes(self) -> u64 {
        self.max_encoded_bytes.min(
            self.max_decoded_bytes
                .min(self.max_record_bytes)
                .saturating_add(crate::SAVE_HEADER_SIZE as u64),
        )
    }

    pub(crate) fn payload_bytes(self) -> u64 {
        self.max_decoded_bytes.min(self.max_record_bytes).min(
            self.max_encoded_bytes
                .saturating_sub(crate::SAVE_HEADER_SIZE as u64),
        )
    }
}

pub(crate) const COLLECTION_LIMIT_ERROR: &str = "save collection exceeds safety limit";

mod codec;
pub(crate) use codec::{check_collections, deserialize_from};
