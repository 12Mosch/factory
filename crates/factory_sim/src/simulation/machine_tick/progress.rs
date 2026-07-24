use super::*;

pub(super) enum ProgressAdvance {
    Blocked,
    InProgress,
    Completed,
}

pub(super) struct MachineProgressAdvance {
    pub result: ProgressAdvance,
    pub consumed_fuel: Option<ItemId>,
}

/// The per-machine environment needed to advance work on either energy
/// source: the catalog resolves burner fuel, the power state gates electric
/// machines.
pub(super) struct MachineEnergyContext<'a> {
    pub catalog: &'a PrototypeCatalog,
    pub power: &'a PowerSubsystem,
    pub electric_consumers: &'a mut BTreeMap<EntityId, ElectricConsumerState>,
    pub entity_id: EntityId,
    pub energy_multiplier_permyriad: u64,
}

/// Advances one tick of work on a machine driven by either energy source:
/// burners consume stored fuel energy, electric machines are gated on their
/// power network's satisfaction.
pub(super) fn advance_machine_progress<P: TickProfiler>(
    context: MachineEnergyContext<'_>,
    energy: &mut MachineEnergy,
    progress_ticks: &mut u32,
    required_ticks: u32,
    profiler: &mut P,
) -> MachineProgressAdvance {
    match energy {
        MachineEnergy::Burner(burner) => advance_burner_progress(
            context.catalog,
            burner,
            progress_ticks,
            required_ticks,
            context.energy_multiplier_permyriad,
            profiler,
        ),
        MachineEnergy::Electric => {
            let result = if electric_work_allowed_for(
                context.power,
                context.electric_consumers,
                context.entity_id,
            ) {
                advance_electric_progress(progress_ticks, required_ticks)
            } else {
                ProgressAdvance::Blocked
            };
            MachineProgressAdvance {
                result,
                consumed_fuel: None,
            }
        }
    }
}

pub(super) fn advance_burner_progress<P: TickProfiler>(
    catalog: &PrototypeCatalog,
    energy: &mut BurnerEnergy,
    progress_ticks: &mut u32,
    required_ticks: u32,
    energy_multiplier_permyriad: u64,
    profiler: &mut P,
) -> MachineProgressAdvance {
    let mut consumed_fuel = None;
    let joules_per_tick = energy.energy_usage_watts * energy_multiplier_permyriad as f64
        / 10_000.0
        / FIXED_SIM_TICKS_PER_SECOND_F64;
    if energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
        consumed_fuel = profiler.measure(ProfilePhase::InventoryTransfers, || {
            try_consume_fuel(catalog, energy)
        });
        if consumed_fuel.is_none()
            || energy.energy_remaining_joules + f64::EPSILON < joules_per_tick
        {
            return MachineProgressAdvance {
                result: ProgressAdvance::Blocked,
                consumed_fuel,
            };
        }
    }

    energy.energy_remaining_joules -= joules_per_tick;
    *progress_ticks += 1;

    if *progress_ticks < required_ticks {
        MachineProgressAdvance {
            result: ProgressAdvance::InProgress,
            consumed_fuel,
        }
    } else {
        *progress_ticks = 0;
        MachineProgressAdvance {
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
