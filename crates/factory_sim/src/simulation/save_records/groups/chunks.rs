//! Chunk records: one record per world chunk (`chunk/<x>/<y>`).
//!
//! Terrain is the only partitioned record family: every other global keeps
//! explicit ownership with stable references. Chunk keys must be canonical —
//! formatting the parsed coordinates reproduces the key — so aliases such as
//! `chunk/01/2` cannot load under a key that selective access and re-saving
//! would not reproduce.

use super::super::super::*;
use super::super::codec::*;
use super::super::registry::*;
use super::{PartialSnapshot, check_record_schema};

pub(super) fn encode(chunk: &Chunk, limits: crate::SaveLimits) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(chunk, limits)
}

pub(super) fn measure(chunk: &Chunk, limits: crate::SaveLimits) -> Result<u64, SaveLoadError> {
    super::measure_tuple(chunk, limits)
}

pub(super) fn decode(
    partial: &mut PartialSnapshot,
    entry: &ManifestEntry,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    let Some(coord) = parse_chunk_key(&entry.key) else {
        if entry.required {
            return Err(record_error(format!(
                "record {:?} is required but unknown to this build",
                entry.key
            )));
        }
        // Unknown optional records were already bounds- and
        // checksum-verified; their payloads need no decoding.
        return Ok(());
    };
    check_record_schema(entry)?;
    if chunk_key(coord) != entry.key {
        return Err(record_error(format!(
            "record {:?} is not a canonical chunk key",
            entry.key
        )));
    }
    let chunk: Chunk = decode_group(&entry.key, payload, limits)?;
    if chunk.coord != coord {
        return Err(record_error(format!(
            "record {:?} carries chunk at ({}, {})",
            entry.key, chunk.coord.x, chunk.coord.y
        )));
    }
    if partial.chunks.insert(coord, chunk).is_some() {
        return Err(record_error(format!(
            "record index carries duplicate chunk {coord:?}"
        )));
    }
    Ok(())
}
