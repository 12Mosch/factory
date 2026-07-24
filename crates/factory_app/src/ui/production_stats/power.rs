use factory_sim::{PowerNetworkSnapshot, PowerStatisticsSample, PowerSummary};

use crate::ui::debug_overlay::{format_joules, format_watts};
use crate::ui::production_stats::PowerGraphPoint;

pub fn power_summary_lines(
    summary: PowerSummary,
    networks: &[PowerNetworkSnapshot],
) -> Vec<String> {
    let mut lines = vec![
        format!("Production: {}", format_watts(summary.production_watts)),
        format!(
            "Available: {}",
            format_watts(summary.available_production_watts)
        ),
        format!("Consumption: {}", format_watts(summary.consumption_watts)),
        format!(
            "Satisfaction: {:.1}%",
            f64::from(summary.satisfaction_permyriad) / 100.0
        ),
        format!("Networks: {}", summary.network_count),
        format!(
            "Accumulators: {} (charge {}, discharge {})",
            summary.accumulator_count,
            format_watts(summary.accumulator_charge_watts),
            format_watts(summary.accumulator_discharge_watts),
        ),
        format!(
            "Stored energy: {} / {}",
            format_joules(summary.accumulator_stored_energy_joules),
            format_joules(summary.accumulator_capacity_joules),
        ),
    ];
    lines.extend(networks.iter().map(|network| {
        format!(
            "Network {}: poles {}, producers {}, consumers {}, prod {}, avail {}, cons {}, sat {:.1}%, accs {} (chg {}, dis {}, {} / {})",
            network.network_id,
            network.pole_count,
            network.producer_count,
            network.consumer_count,
            format_watts(network.production_watts),
            format_watts(network.available_production_watts),
            format_watts(network.consumption_watts),
            f64::from(network.satisfaction_permyriad) / 100.0,
            network.accumulator_count,
            format_watts(network.accumulator_charge_watts),
            format_watts(network.accumulator_discharge_watts),
            format_joules(network.accumulator_stored_energy_joules),
            format_joules(network.accumulator_capacity_joules),
        )
    }));
    lines
}

pub fn power_graph_points(
    samples: &[PowerStatisticsSample],
    max_points: usize,
) -> Vec<PowerGraphPoint> {
    if samples.is_empty() || max_points == 0 {
        return Vec::new();
    }
    if samples.len() <= max_points {
        return samples
            .iter()
            .map(|sample| PowerGraphPoint {
                production_watts: sample.production_watts,
                consumption_watts: sample.consumption_watts,
            })
            .collect();
    }

    let chunk_size = samples.len().div_ceil(max_points);
    samples
        .chunks(chunk_size)
        .map(|chunk| PowerGraphPoint {
            production_watts: chunk
                .iter()
                .map(|sample| sample.production_watts)
                .max()
                .unwrap_or(0),
            consumption_watts: chunk
                .iter()
                .map(|sample| sample.consumption_watts)
                .max()
                .unwrap_or(0),
        })
        .collect()
}
