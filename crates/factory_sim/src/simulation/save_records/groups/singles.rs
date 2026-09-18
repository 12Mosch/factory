//! Single-field records: prototypes, chart, chunk queue, entities, and
//! construction.
//!
//! These groups carry exactly one registry field each, so they encode the
//! borrowed field directly instead of building a tuple. They live together
//! because none of them needs tuple-order documentation of its own.

use super::super::super::*;
use super::super::codec::*;
use super::{BorrowedRecordFields, PartialSnapshot};

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
