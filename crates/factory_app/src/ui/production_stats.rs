mod components;
mod diagnostics;
mod power;
mod rows;
mod snapshot;
mod systems;
mod view;

pub use components::{ItemStatDisplayRow, PowerGraphPoint, ProductionStatsTabButton};
pub use diagnostics::{bottleneck_lines, diagnostic_lines};
pub use power::{power_graph_points, power_summary_lines};
pub use rows::{
    consumption_rows, fluid_consumption_rows, fluid_production_rows, format_fluid_per_minute,
    format_per_minute_u64, production_rows,
};

pub(crate) use systems::{handle_production_stats_buttons, sync_production_stats_window};
