//! Trains record: rolling stock plus unfinished route searches.
//!
//! A train mid-plan through a save is still mid-plan when it loads: the
//! routing cursor and the serialized A* frontiers of searches larger than
//! one tick's expansion slice are durable state, not a cache.

use super::super::super::*;
use super::super::codec::*;
use super::{BorrowedRecordFields, PartialSnapshot};

pub(super) type TrainsTuple = (
    RollingStockSubsystem,
    BTreeMap<TrainId, rolling_stock_ops::PendingTrainRouteSearch>,
);

fn tuple<'a>(
    fields: &'a BorrowedRecordFields<'a>,
) -> (
    &'a RollingStockSubsystem,
    &'a BTreeMap<TrainId, rolling_stock_ops::PendingTrainRouteSearch>,
) {
    (fields.rolling_stock, fields.pending_train_route_searches)
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
    partial.trains = Some(decode_group(key, payload, limits)?);
    Ok(())
}
