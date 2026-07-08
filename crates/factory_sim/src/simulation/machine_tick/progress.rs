use super::*;

pub(super) enum ProgressAdvance {
    Blocked,
    InProgress,
    Completed,
}

pub(super) struct BurnerProgressAdvance {
    pub result: ProgressAdvance,
    pub consumed_fuel: Option<ItemId>,
}

pub(super) fn advance_burner_progress<P: TickProfiler>(
    catalog: &PrototypeCatalog,
    energy: &mut BurnerEnergy,
    progress_ticks: &mut u32,
    required_ticks: u32,
    profiler: &mut P,
) -> BurnerProgressAdvance {
    let mut consumed_fuel = None;
    let joules_per_tick = energy.energy_usage_watts / FIXED_SIM_TICKS_PER_SECOND_F64;
    if energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
        consumed_fuel = profiler.measure(ProfilePhase::InventoryTransfers, || {
            try_consume_fuel(catalog, energy)
        });
        if consumed_fuel.is_none()
            || energy.energy_remaining_joules + f64::EPSILON < joules_per_tick
        {
            return BurnerProgressAdvance {
                result: ProgressAdvance::Blocked,
                consumed_fuel,
            };
        }
    }

    energy.energy_remaining_joules -= joules_per_tick;
    *progress_ticks += 1;

    if *progress_ticks < required_ticks {
        BurnerProgressAdvance {
            result: ProgressAdvance::InProgress,
            consumed_fuel,
        }
    } else {
        *progress_ticks = 0;
        BurnerProgressAdvance {
            result: ProgressAdvance::Completed,
            consumed_fuel,
        }
    }
}

pub(super) fn advance_electric_progress(
    progress_ticks: &mut u32,
    required_ticks: u32,
) -> ProgressAdvance {
    *progress_ticks += 1;
    if *progress_ticks < required_ticks {
        ProgressAdvance::InProgress
    } else {
        *progress_ticks = 0;
        ProgressAdvance::Completed
    }
}
