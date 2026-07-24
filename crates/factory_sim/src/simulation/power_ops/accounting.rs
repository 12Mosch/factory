use super::types::NetworkPowerBalance;
use super::*;

pub(super) fn power_satisfaction(available_watts: u64, demand_watts: u64) -> (u64, u32) {
    if demand_watts == 0 {
        return (0, POWER_SATISFACTION_FULL_PERMYRIAD);
    }
    if available_watts >= demand_watts {
        return (demand_watts, POWER_SATISFACTION_FULL_PERMYRIAD);
    }

    let satisfaction =
        available_watts.saturating_mul(u64::from(POWER_SATISFACTION_FULL_PERMYRIAD)) / demand_watts;
    (available_watts, satisfaction as u32)
}

/// Resolves one network's regular generation and accumulator dispatch once
/// solar and steam capability are known. Regular generation (solar first, then
/// steam) serves ordinary demand before storage: any residual deficit is met by
/// discharging accumulators, while any surplus up to the charge target is used
/// to charge them. A network therefore never charges and discharges in the same
/// tick.
pub(super) fn solve_network_storage_balance(network: &mut NetworkPowerBalance) {
    let target = network
        .consumption_watts
        .saturating_add(network.charge_capability_watts);
    let regular_available = network
        .solar_watts
        .saturating_add(network.steam_available_watts);
    let regular_used = regular_available.min(target);
    let regular_to_demand = regular_used.min(network.consumption_watts);

    let deficit = network.consumption_watts.saturating_sub(regular_to_demand);
    network.discharge_watts = deficit.min(network.discharge_capability_watts);
    network.charge_watts = regular_used
        .saturating_sub(network.consumption_watts)
        .min(network.charge_capability_watts);

    network.production_watts = regular_to_demand.saturating_add(network.discharge_watts);
    network.available_production_watts =
        regular_available.saturating_add(network.discharge_capability_watts);
    let (_, satisfaction_permyriad) =
        power_satisfaction(network.production_watts, network.consumption_watts);
    network.satisfaction_permyriad = satisfaction_permyriad;
}

pub(super) fn refresh_network_snapshots(
    networks: &[NetworkPowerBalance],
    snapshots: &mut Vec<PowerNetworkSnapshot>,
) -> bool {
    let map_changed = snapshots.len() != networks.len()
        || snapshots
            .iter()
            .zip(networks)
            .any(|(previous, next)| previous.satisfaction_permyriad != next.satisfaction_permyriad);
    snapshots.clear();
    snapshots.extend(networks.iter().enumerate().map(|(network_id, network)| {
        PowerNetworkSnapshot {
            network_id: network_id as u32,
            pole_count: network.pole_count,
            producer_count: network.producer_count,
            consumer_count: network.consumer_count,
            production_watts: network.production_watts,
            available_production_watts: network.available_production_watts,
            consumption_watts: network.consumption_watts,
            satisfaction_permyriad: network.satisfaction_permyriad,
            accumulator_count: network.accumulator_count,
            accumulator_charge_watts: network.charge_watts,
            accumulator_discharge_watts: network.discharge_watts,
            accumulator_stored_energy_joules: network.stored_energy_joules,
            accumulator_capacity_joules: network.capacity_joules,
        }
    }));
    map_changed
}

pub(super) fn aggregate_power_summary(networks: &[PowerNetworkSnapshot]) -> PowerSummary {
    let production_watts = networks
        .iter()
        .map(|network| network.production_watts)
        .sum::<u64>();
    let available_production_watts = networks
        .iter()
        .map(|network| network.available_production_watts)
        .sum::<u64>();
    let consumption_watts = networks
        .iter()
        .map(|network| network.consumption_watts)
        .sum::<u64>();
    let satisfaction_permyriad = if consumption_watts == 0 {
        POWER_SATISFACTION_FULL_PERMYRIAD
    } else {
        production_watts
            .saturating_mul(u64::from(POWER_SATISFACTION_FULL_PERMYRIAD))
            .checked_div(consumption_watts)
            .unwrap_or(u64::from(POWER_SATISFACTION_FULL_PERMYRIAD)) as u32
    };

    let accumulator_count = networks
        .iter()
        .map(|network| network.accumulator_count)
        .sum();
    let accumulator_charge_watts = networks
        .iter()
        .map(|network| network.accumulator_charge_watts)
        .fold(0u64, u64::saturating_add);
    let accumulator_discharge_watts = networks
        .iter()
        .map(|network| network.accumulator_discharge_watts)
        .fold(0u64, u64::saturating_add);
    let accumulator_stored_energy_joules = networks
        .iter()
        .map(|network| network.accumulator_stored_energy_joules)
        .fold(0u64, u64::saturating_add);
    let accumulator_capacity_joules = networks
        .iter()
        .map(|network| network.accumulator_capacity_joules)
        .fold(0u64, u64::saturating_add);

    PowerSummary {
        production_watts,
        available_production_watts,
        consumption_watts,
        satisfaction_permyriad,
        network_count: networks.len(),
        accumulator_count,
        accumulator_charge_watts,
        accumulator_discharge_watts,
        accumulator_stored_energy_joules,
        accumulator_capacity_joules,
    }
}
