//! Core record: tick, world seed, day/night phase, config, revisions.
//!
//! The smallest subsystem module: it shows the pattern every other group
//! follows. `tuple` builds the borrowed wire value in the documented order
//! (field order here is the wire order); `encode`/`measure` share it so
//! sizing and encoding can never diverge; `decode` fills the assembly slot.

use super::super::super::*;
use super::super::codec::*;
use super::{BorrowedRecordFields, PartialSnapshot, measure_tuple};

/// Wire order: tick, world seed, day/night phase, config,
/// entity-topology revision, chunk revision, walkability revision.
pub(super) type CoreTuple = (
    u64,
    u64,
    Option<DayNightCycleState>,
    SimulationConfig,
    u64,
    u64,
    u64,
);

fn tuple<'a>(
    fields: &'a BorrowedRecordFields<'a>,
) -> (
    &'a u64,
    &'a u64,
    &'a Option<DayNightCycleState>,
    &'a SimulationConfig,
    &'a u64,
    &'a u64,
    &'a u64,
) {
    (
        &fields.tick,
        &fields.world_seed,
        fields.day_night_cycle,
        fields.config,
        &fields.entity_topology_revision,
        &fields.world_chunk_revision,
        &fields.world_walkability_revision,
    )
}

pub(super) fn encode(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(&tuple(fields), limits)
}

pub(super) fn measure(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    measure_tuple(&tuple(fields), limits)
}

pub(super) fn decode(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.core = Some(decode_group(key, payload, limits)?);
    Ok(())
}
