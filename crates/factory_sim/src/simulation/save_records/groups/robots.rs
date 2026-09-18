//! Robots record: robot networks, logistic work, and flights.

use super::super::super::robot_ops::RobotLogisticWorkState;
use super::super::super::*;
use super::super::codec::*;
use super::{BorrowedRecordFields, PartialSnapshot};

pub(super) type RobotsTuple = (
    Vec<RobotNetworkSnapshot>,
    RobotLogisticWorkState,
    RobotFlightSubsystem,
);

fn tuple<'a>(
    fields: &'a BorrowedRecordFields<'a>,
) -> (
    &'a Vec<RobotNetworkSnapshot>,
    &'a RobotLogisticWorkState,
    &'a RobotFlightSubsystem,
) {
    (
        fields.robot_networks,
        fields.robot_logistic_work,
        fields.robot_flights,
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
    partial.robots = Some(decode_group(key, payload, limits)?);
    Ok(())
}
