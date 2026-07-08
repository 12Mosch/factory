use factory_sim::{FluidStatisticsRow, ItemStatisticsRow, Simulation};

use crate::ui::formatting::{format_fluid_display_name, format_item_display_name};
use crate::ui::production_stats::ItemStatDisplayRow;

enum StatDirection {
    Produced,
    Consumed,
}

pub fn production_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    item_stat_rows(sim, StatDirection::Produced)
}

pub fn consumption_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    item_stat_rows(sim, StatDirection::Consumed)
}

pub fn fluid_production_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    fluid_stat_rows(sim, StatDirection::Produced)
}

pub fn fluid_consumption_rows(sim: &Simulation) -> Vec<ItemStatDisplayRow> {
    fluid_stat_rows(sim, StatDirection::Consumed)
}

fn item_stat_rows(sim: &Simulation, direction: StatDirection) -> Vec<ItemStatDisplayRow> {
    let mut rows = sim.item_statistics().rows;
    rows.sort_by(|a, b| {
        item_last_minute(b, &direction)
            .cmp(&item_last_minute(a, &direction))
            .then_with(|| item_name(sim, a).cmp(&item_name(sim, b)))
    });
    rows.into_iter()
        .filter(|row| item_last_minute(row, &direction) > 0 || item_total(row, &direction) > 0)
        .map(|row| ItemStatDisplayRow {
            item_name: format_item_display_name(sim.catalog(), row.item_id),
            per_minute: format_per_minute(item_last_minute(&row, &direction)),
            total: item_total(&row, &direction).to_string(),
        })
        .collect()
}

fn fluid_stat_rows(sim: &Simulation, direction: StatDirection) -> Vec<ItemStatDisplayRow> {
    let mut rows = sim.fluid_statistics().rows;
    rows.sort_by(|a, b| {
        fluid_last_minute(b, &direction)
            .cmp(&fluid_last_minute(a, &direction))
            .then_with(|| fluid_name(sim, a).cmp(&fluid_name(sim, b)))
    });
    rows.into_iter()
        .filter(|row| fluid_last_minute(row, &direction) > 0 || fluid_total(row, &direction) > 0)
        .map(|row| ItemStatDisplayRow {
            item_name: format_fluid_display_name(sim.catalog(), row.fluid_id),
            per_minute: format_fluid_per_minute(fluid_last_minute(&row, &direction)),
            total: format_fluid_amount(fluid_total(&row, &direction)),
        })
        .collect()
}

fn item_last_minute(row: &ItemStatisticsRow, direction: &StatDirection) -> u64 {
    match direction {
        StatDirection::Produced => row.produced_last_minute,
        StatDirection::Consumed => row.consumed_last_minute,
    }
}

fn item_total(row: &ItemStatisticsRow, direction: &StatDirection) -> u64 {
    match direction {
        StatDirection::Produced => row.produced_total,
        StatDirection::Consumed => row.consumed_total,
    }
}

fn fluid_last_minute(row: &FluidStatisticsRow, direction: &StatDirection) -> u64 {
    match direction {
        StatDirection::Produced => row.produced_last_minute,
        StatDirection::Consumed => row.consumed_last_minute,
    }
}

fn fluid_total(row: &FluidStatisticsRow, direction: &StatDirection) -> u64 {
    match direction {
        StatDirection::Produced => row.produced_total,
        StatDirection::Consumed => row.consumed_total,
    }
}

fn item_name(sim: &Simulation, row: &ItemStatisticsRow) -> String {
    format_item_display_name(sim.catalog(), row.item_id)
}

fn fluid_name(sim: &Simulation, row: &FluidStatisticsRow) -> String {
    format_fluid_display_name(sim.catalog(), row.fluid_id)
}

pub fn format_per_minute_u64(value: u64) -> String {
    format!("{value}/min")
}

fn format_per_minute(value: u64) -> String {
    format_per_minute_u64(value)
}

pub fn format_fluid_per_minute(milliunits: u64) -> String {
    format!("{}/min", format_fluid_amount(milliunits))
}

fn format_fluid_amount(milliunits: u64) -> String {
    let whole = milliunits / 1_000;
    let remainder = milliunits % 1_000;
    if remainder == 0 {
        whole.to_string()
    } else {
        let tenths = (remainder / 100).min(9);
        format!("{whole}.{tenths}")
    }
}
