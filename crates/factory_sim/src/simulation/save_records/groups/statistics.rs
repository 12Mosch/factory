//! Statistics record: item/fluid/power statistics, launches, deaths.

use super::super::super::*;
use super::super::codec::*;
use super::super::registry::KEY_STATISTICS;
use super::{BorrowedRecordFields, PartialSnapshot, RecordHandler};

pub(super) type StatisticsTuple = (ItemStatistics, FluidStatistics, PowerStatistics, u64, u64);

fn tuple<'a>(
    fields: &'a BorrowedRecordFields<'a>,
) -> (
    &'a ItemStatistics,
    &'a FluidStatistics,
    &'a PowerStatistics,
    &'a u64,
    &'a u64,
) {
    (
        fields.item_statistics,
        fields.fluid_statistics,
        fields.power_statistics,
        &fields.rockets_launched,
        &fields.player_deaths,
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
    partial.statistics = Some(decode_group(key, payload, limits)?);
    Ok(())
}

/// This record's table row: the stable key travels with its own handlers,
/// so no assembly site can pair a key with another subsystem's codecs.
pub(super) const HANDLER: RecordHandler = RecordHandler {
    key: KEY_STATISTICS,
    required: true,
    encode,
    measure,
    decode,
};
