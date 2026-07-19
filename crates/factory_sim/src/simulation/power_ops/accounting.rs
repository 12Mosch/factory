use super::types::NetworkAccumulator;
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

pub(super) fn refresh_network_snapshots(
    networks: &[NetworkAccumulator],
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

    PowerSummary {
        production_watts,
        available_production_watts,
        consumption_watts,
        satisfaction_permyriad,
        network_count: networks.len(),
    }
}
