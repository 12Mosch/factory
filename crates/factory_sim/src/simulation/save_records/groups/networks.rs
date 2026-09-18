//! Network records: power, fluids, and heat.
//!
//! The three energy/flow networks share one module because they follow the
//! same shape — a snapshot plus a pending-invalidation flag — and evolve
//! together. Each network keeps explicit global ownership with stable
//! references; none is forced into terrain-file boundaries.

use super::super::super::*;
use super::super::codec::*;
use super::super::registry::{KEY_FLUIDS, KEY_HEAT, KEY_POWER};
use super::{BorrowedRecordFields, PartialSnapshot, RecordHandler};

/// One table row per network below. Each key travels with its own handlers,
/// so no assembly site can pair a key with another subsystem's codecs.
pub(super) const POWER: RecordHandler = RecordHandler {
    key: KEY_POWER,
    required: true,
    encode: encode_power,
    measure: measure_power,
    decode: decode_power,
};

pub(super) const FLUIDS: RecordHandler = RecordHandler {
    key: KEY_FLUIDS,
    required: true,
    encode: encode_fluids,
    measure: measure_fluids,
    decode: decode_fluids,
};

pub(super) const HEAT: RecordHandler = RecordHandler {
    key: KEY_HEAT,
    required: true,
    encode: encode_heat,
    measure: measure_heat,
    decode: decode_heat,
};

pub(super) type PowerTuple = (
    PowerSummary,
    Vec<PowerNetworkSnapshot>,
    DenseEntityMap<EntityPowerStatus>,
);
pub(super) type FluidsTuple = (Vec<FluidNetworkSnapshot>, bool);
pub(super) type HeatTuple = (Vec<HeatNetworkSnapshot>, bool);

pub(super) fn encode_power(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(
        &(
            fields.power_summary,
            fields.power_networks,
            fields.entity_power_statuses,
        ),
        limits,
    )
}

pub(super) fn measure_power(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(
        &(
            fields.power_summary,
            fields.power_networks,
            fields.entity_power_statuses,
        ),
        limits,
    )
}

pub(super) fn decode_power(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.power = Some(decode_group(key, payload, limits)?);
    Ok(())
}

pub(super) fn encode_fluids(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(
        &(fields.fluid_networks, fields.fluid_topology_dirty),
        limits,
    )
}

pub(super) fn measure_fluids(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(
        &(fields.fluid_networks, fields.fluid_topology_dirty),
        limits,
    )
}

pub(super) fn decode_fluids(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.fluids = Some(decode_group(key, payload, limits)?);
    Ok(())
}

pub(super) fn encode_heat(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<Vec<u8>, SaveLoadError> {
    encode_group(&(fields.heat_networks, fields.heat_topology_dirty), limits)
}

pub(super) fn measure_heat(
    fields: &BorrowedRecordFields<'_>,
    limits: crate::SaveLimits,
) -> Result<u64, SaveLoadError> {
    super::measure_tuple(&(fields.heat_networks, fields.heat_topology_dirty), limits)
}

pub(super) fn decode_heat(
    partial: &mut PartialSnapshot,
    key: &str,
    payload: &[u8],
    limits: crate::SaveLimits,
) -> Result<(), SaveLoadError> {
    partial.heat = Some(decode_group(key, payload, limits)?);
    Ok(())
}
