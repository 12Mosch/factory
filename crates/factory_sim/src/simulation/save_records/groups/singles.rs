//! Single-field records: prototypes, chart, chunk queue, entities, and
//! construction.
//!
//! These groups carry exactly one registry field each, so they encode the
//! borrowed field directly instead of building a tuple. They live together
//! because none of them needs tuple-order documentation of its own.

use super::super::super::*;
use super::super::codec::*;
use super::super::registry::{
    KEY_CHART, KEY_CHUNK_QUEUE, KEY_CONSTRUCTION, KEY_ENTITIES, KEY_PROTOTYPES,
};
use super::{BorrowedRecordFields, PartialSnapshot, RecordHandler};

/// One table row per single-field record below. Each key travels with its
/// own handlers, so no assembly site can pair a key with another
/// subsystem's codecs.
pub(super) const PROTOTYPES: RecordHandler = RecordHandler {
    key: KEY_PROTOTYPES,
    required: true,
    encode: encode_prototypes,
    measure: measure_prototypes,
    decode: decode_prototypes,
};

pub(super) const CHART: RecordHandler = RecordHandler {
    key: KEY_CHART,
    required: true,
    encode: encode_chart,
    measure: measure_chart,
    decode: decode_chart,
};

pub(super) const CHUNK_QUEUE: RecordHandler = RecordHandler {
    key: KEY_CHUNK_QUEUE,
    required: true,
    encode: encode_chunk_queue,
    measure: measure_chunk_queue,
    decode: decode_chunk_queue,
};

pub(super) const ENTITIES: RecordHandler = RecordHandler {
    key: KEY_ENTITIES,
    required: true,
    encode: encode_entities,
    measure: measure_entities,
    decode: decode_entities,
};

pub(super) const CONSTRUCTION: RecordHandler = RecordHandler {
    key: KEY_CONSTRUCTION,
    required: true,
    encode: encode_construction,
    measure: measure_construction,
    decode: decode_construction,
};

pub(super) fn encode_prototypes(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(fields.prototypes, limits)
}

pub(super) fn measure_prototypes(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(fields.prototypes, limits)
}

pub(super) fn decode_prototypes(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.prototypes = Some(decode_group(key, payload, limits)?);
    Ok(())
}

pub(super) fn encode_chart(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(fields.chart, limits)
}

pub(super) fn measure_chart(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(fields.chart, limits)
}

pub(super) fn decode_chart(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.chart = Some(decode_group(key, payload, limits)?);
    Ok(())
}

pub(super) fn encode_chunk_queue(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(fields.chunk_generation_queue, limits)
}

pub(super) fn measure_chunk_queue(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(fields.chunk_generation_queue, limits)
}

pub(super) fn decode_chunk_queue(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.chunk_queue = Some(decode_group(key, payload, limits)?);
    Ok(())
}

pub(super) fn encode_entities(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(fields.entities, limits)
}

pub(super) fn measure_entities(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(fields.entities, limits)
}

pub(super) fn decode_entities(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.entities = Some(decode_group(key, payload, limits)?);
    Ok(())
}

pub(super) fn encode_construction(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(fields.construction, limits)
}

pub(super) fn measure_construction(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(fields.construction, limits)
}

pub(super) fn decode_construction(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.construction = Some(decode_group(key, payload, limits)?);
    Ok(())
}
