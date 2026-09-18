//! Environment record: pollution, enemies, and the transport cache with
//! its scheduling state.
//!
//! Navigation frontiers, target decisions, and belt identity cursors are
//! durable because rebuilding them would spend future ticks or change
//! movement timing — a cold cache and a warm cache do not produce the same
//! next tick here.

use super::super::super::*;
use super::super::codec::*;
use super::super::registry::KEY_ENVIRONMENT;
use super::{BorrowedRecordFields, PartialSnapshot, RecordHandler};

pub(super) type EnvironmentTuple = (
    PollutionState,
    EnemySubsystem,
    TransportLaneCache,
    enemy::EnemyNavigation,
    enemy::AttackTargetCache,
);

#[allow(clippy::type_complexity)]
fn tuple<'a>(
    fields: &'a BorrowedRecordFields<'a>,
) -> (
    &'a PollutionState,
    &'a EnemySubsystem,
    &'a TransportLaneCache,
    &'a enemy::EnemyNavigation,
    &'a enemy::AttackTargetCache,
) {
    (
        fields.pollution,
        fields.enemies,
        fields.transport,
        fields.enemy_navigation,
        fields.attack_targets,
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
    super::measure_tuple(&tuple(fields), limits)
}

pub(super) fn decode(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.environment = Some(decode_group(key, payload, limits)?);
    Ok(())
}

/// This record's table row: the stable key travels with its own handlers,
/// so no assembly site can pair a key with another subsystem's codecs.
pub(super) const HANDLER: RecordHandler = RecordHandler {
    key: KEY_ENVIRONMENT,
    required: true,
    encode,
    measure,
    decode,
};
