use crate::simulation::{FIXED_SIM_TICKS_PER_SECOND_F64, FluidId};

pub(in crate::simulation) fn per_tick_milliunits(per_second_milliunits: u64) -> u64 {
    ceil_div_u64(per_second_milliunits, FIXED_SIM_TICKS_PER_SECOND_F64 as u64)
}

pub(in crate::simulation) fn ceil_div_u64(numerator: u64, denominator: u64) -> u64 {
    if numerator == 0 {
        0
    } else {
        numerator.div_ceil(denominator)
    }
}

pub(super) fn proportional_amount(total: u64, capacity: u64, total_capacity: u64) -> u64 {
    if total_capacity == 0 {
        return 0;
    }

    ((u128::from(total) * u128::from(capacity)) / u128::from(total_capacity)) as u64
}

pub(super) fn single_fluid(mut fluids: impl Iterator<Item = FluidId>) -> Option<FluidId> {
    let first = fluids.next()?;
    fluids.next().is_none().then_some(first)
}

pub(super) fn fluid_filter_accepts(filter: Option<FluidId>, fluid_id: FluidId) -> bool {
    filter.is_none_or(|filter| filter == fluid_id)
}
