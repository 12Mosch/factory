use factory_sim::Simulation;

use crate::ui::production_stats::components::ProductionStatsSnapshot;
use crate::ui::production_stats::diagnostics::{bottleneck_lines, diagnostic_lines};
use crate::ui::production_stats::power::{power_graph_points, power_summary_lines};
use crate::ui::production_stats::rows::{
    consumption_rows, fluid_consumption_rows, fluid_production_rows, production_rows,
};
use crate::ui::resources::StatsTab;

const POWER_GRAPH_POINT_COUNT: usize = 40;

pub(crate) fn production_stats_snapshot(
    sim: &Simulation,
    selected_tab: StatsTab,
) -> ProductionStatsSnapshot {
    match selected_tab {
        StatsTab::Production => ProductionStatsSnapshot {
            selected_tab,
            item_rows: production_rows(sim),
            fluid_rows: fluid_production_rows(sim),
            power_lines: Vec::new(),
            power_graph: Vec::new(),
            diagnostic_lines: Vec::new(),
            bottleneck_lines: Vec::new(),
        },
        StatsTab::Consumption => ProductionStatsSnapshot {
            selected_tab,
            item_rows: consumption_rows(sim),
            fluid_rows: fluid_consumption_rows(sim),
            power_lines: Vec::new(),
            power_graph: Vec::new(),
            diagnostic_lines: Vec::new(),
            bottleneck_lines: Vec::new(),
        },
        StatsTab::Power => ProductionStatsSnapshot {
            selected_tab,
            item_rows: Vec::new(),
            fluid_rows: Vec::new(),
            power_lines: power_summary_lines(sim.power_summary(), sim.power_networks()),
            power_graph: power_graph_points(
                &sim.power_statistics().samples,
                POWER_GRAPH_POINT_COUNT,
            ),
            diagnostic_lines: Vec::new(),
            bottleneck_lines: Vec::new(),
        },
        StatsTab::Diagnostics => ProductionStatsSnapshot {
            selected_tab,
            item_rows: Vec::new(),
            fluid_rows: Vec::new(),
            power_lines: Vec::new(),
            power_graph: Vec::new(),
            diagnostic_lines: diagnostic_lines(sim),
            bottleneck_lines: bottleneck_lines(sim),
        },
    }
}
