use bevy::prelude::*;
use factory_data::{PrototypeCatalog, TechnologyEffect, TechnologyId};

use crate::ui::formatting::{format_item_display_name, format_recipe_display_name};
use crate::ui::resources::TechnologyWindowState;

use super::components::{TechnologyPanelSnapshot, TechnologyUiState};

pub(crate) fn technology_panel_snapshot(
    sim: &factory_sim::Simulation,
    window_state: &TechnologyWindowState,
) -> TechnologyPanelSnapshot {
    TechnologyPanelSnapshot {
        selected: window_state.selected,
        active: sim.active_research(),
        queue: sim.research_queue().to_vec(),
        progress_units: sim
            .catalog()
            .technologies
            .iter()
            .map(|technology| sim.technology_progress(technology.id).unwrap_or(0))
            .collect(),
        completed_levels: sim
            .catalog()
            .technologies
            .iter()
            .map(|technology| sim.technology_level(technology.id).unwrap_or(0))
            .collect(),
    }
}

pub(crate) fn active_research_text(sim: &factory_sim::Simulation) -> String {
    let Some(technology_id) = sim.active_research() else {
        return "Active Research: <none>".to_string();
    };

    format!(
        "Active Research: {} ({})",
        technology_name(sim.catalog(), technology_id),
        technology_progress_text(sim, technology_id)
    )
}

pub(crate) fn queue_text(sim: &factory_sim::Simulation) -> String {
    if sim.research_queue().is_empty() {
        return "Queue: <empty>".to_string();
    }

    format!(
        "Queue: {}",
        sim.research_queue()
            .iter()
            .map(|technology_id| technology_name(sim.catalog(), *technology_id))
            .collect::<Vec<_>>()
            .join(" -> ")
    )
}

pub(crate) fn technology_progress_text(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> String {
    let progress = sim.technology_progress(technology_id).unwrap_or(0);
    let required = sim
        .technology_next_required_units(technology_id)
        .or_else(|| {
            sim.catalog()
                .technology(technology_id)
                .map(|technology| technology.required_units)
        })
        .unwrap_or(0);
    format!("{progress}/{required}")
}

pub(crate) fn technology_ui_state(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> TechnologyUiState {
    if sim.active_research() == Some(technology_id) {
        TechnologyUiState::Researching
    } else if sim.research_queue().contains(&technology_id) {
        TechnologyUiState::Queued
    } else if sim.is_technology_unlocked(technology_id)
        && sim
            .catalog()
            .technology(technology_id)
            .is_some_and(|technology| !technology.level_model.is_repeatable())
    {
        TechnologyUiState::Researched
    } else if prerequisites_researched(sim, technology_id) {
        TechnologyUiState::Available
    } else {
        TechnologyUiState::Locked
    }
}

pub(crate) fn technology_state_color(state: TechnologyUiState) -> Color {
    match state {
        TechnologyUiState::Researched => Color::srgba(0.12, 0.27, 0.17, 0.98),
        TechnologyUiState::Researching => Color::srgba(0.15, 0.25, 0.34, 0.98),
        TechnologyUiState::Queued => Color::srgba(0.30, 0.24, 0.13, 0.98),
        TechnologyUiState::Available => Color::srgba(0.16, 0.17, 0.17, 0.98),
        TechnologyUiState::Locked => Color::srgba(0.075, 0.078, 0.078, 0.96),
    }
}

pub(crate) fn technology_state_label(state: TechnologyUiState) -> &'static str {
    match state {
        TechnologyUiState::Researched => "Researched",
        TechnologyUiState::Researching => "Researching",
        TechnologyUiState::Queued => "Queued",
        TechnologyUiState::Available => "Available",
        TechnologyUiState::Locked => "Locked",
    }
}

pub(crate) fn prerequisite_text(
    catalog: &PrototypeCatalog,
    technology: &factory_data::TechnologyPrototype,
) -> String {
    if technology.prerequisites.is_empty() {
        return "<none>".to_string();
    }

    technology
        .prerequisites
        .iter()
        .map(|technology_id| technology_name(catalog, *technology_id))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
pub(crate) fn science_cost_text(
    catalog: &PrototypeCatalog,
    technology: &factory_data::TechnologyPrototype,
) -> String {
    let packs = technology
        .science_packs
        .iter()
        .map(|pack| {
            format!(
                "{} x{}",
                format_item_display_name(catalog, pack.item),
                pack.amount
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{packs}; {} units", technology.required_units)
}

pub(crate) fn next_science_cost_text(
    sim: &factory_sim::Simulation,
    technology: &factory_data::TechnologyPrototype,
) -> String {
    let packs = technology
        .science_packs
        .iter()
        .map(|pack| {
            format!(
                "{} x{}",
                format_item_display_name(sim.catalog(), pack.item),
                pack.amount
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let required = sim
        .technology_next_required_units(technology.id)
        .unwrap_or(technology.required_units);
    format!("{packs}; {required} units")
}

#[cfg(test)]
pub(crate) fn unlock_text(
    catalog: &PrototypeCatalog,
    technology: &factory_data::TechnologyPrototype,
) -> String {
    if technology.effects.is_empty() {
        return "<none>".to_string();
    }

    technology
        .effects
        .iter()
        .map(|effect| match *effect {
            TechnologyEffect::UnlockRecipe(recipe_id) => catalog
                .recipe(recipe_id)
                .map(|recipe| format_recipe_display_name(&recipe.name))
                .unwrap_or_else(|| "Unknown".to_string()),
            TechnologyEffect::MiningDrillProductivity { bonus_permyriad } => {
                format!(
                    "+{}% mining-drill productivity per level",
                    bonus_permyriad / 100
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn technology_effect_text(
    sim: &factory_sim::Simulation,
    technology: &factory_data::TechnologyPrototype,
) -> String {
    if technology.effects.is_empty() {
        return "<none>".to_string();
    }
    let completed_levels = sim.technology_level(technology.id).unwrap_or(0);
    technology
        .effects
        .iter()
        .map(|effect| match *effect {
            TechnologyEffect::UnlockRecipe(recipe_id) => sim
                .catalog()
                .recipe(recipe_id)
                .map(|recipe| format_recipe_display_name(&recipe.name))
                .unwrap_or_else(|| "Unknown".to_string()),
            TechnologyEffect::MiningDrillProductivity { bonus_permyriad } => format!(
                "+{}% mining-drill productivity per level; current +{}%, next +{}%",
                bonus_permyriad / 100,
                u64::from(bonus_permyriad) * u64::from(completed_levels) / 100,
                u64::from(bonus_permyriad) * u64::from(completed_levels.saturating_add(1)) / 100,
            ),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn start_queue_label(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> String {
    let repeatable = sim
        .catalog()
        .technology(technology_id)
        .is_some_and(|technology| technology.level_model.is_repeatable());
    if sim.is_technology_unlocked(technology_id) && !repeatable {
        "Researched".to_string()
    } else if sim.active_research() == Some(technology_id) {
        "Researching".to_string()
    } else if sim.research_queue().contains(&technology_id) {
        "Queued".to_string()
    } else if sim.can_enqueue_research(technology_id).is_ok() {
        if sim.active_research().is_some() {
            if repeatable {
                format!(
                    "Queue Level {}",
                    sim.technology_level(technology_id).unwrap_or(0) + 1
                )
            } else {
                "Queue Research".to_string()
            }
        } else if repeatable {
            format!(
                "Research Level {}",
                sim.technology_level(technology_id).unwrap_or(0) + 1
            )
        } else {
            "Start Research".to_string()
        }
    } else {
        "Locked".to_string()
    }
}

pub(crate) fn technology_name(catalog: &PrototypeCatalog, technology_id: TechnologyId) -> String {
    catalog
        .technology(technology_id)
        .map(|technology| format_recipe_display_name(&technology.name))
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn can_enqueue_for_ui(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> bool {
    sim.active_research() != Some(technology_id)
        && !sim.research_queue().contains(&technology_id)
        && sim.can_enqueue_research(technology_id).is_ok()
}

fn prerequisites_researched(sim: &factory_sim::Simulation, technology_id: TechnologyId) -> bool {
    sim.catalog()
        .technology(technology_id)
        .is_some_and(|technology| {
            technology
                .prerequisites
                .iter()
                .all(|prerequisite_id| sim.is_technology_unlocked(*prerequisite_id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_science_technology_cost_formats_all_rgb_packs_and_units() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let technology = catalog
            .technology(factory_data::technology_id_by_name(
                &catalog,
                "production_science_pack",
            ))
            .expect("production science technology should exist");

        assert_eq!(
            science_cost_text(&catalog, technology),
            "Automation Science Pack x1, Logistic Science Pack x1, Chemical Science Pack x1; 100 units"
        );
    }

    #[test]
    fn utility_science_unlocks_format_all_intermediates_and_pack() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let technology = catalog
            .technology(factory_data::technology_id_by_name(
                &catalog,
                "utility_science_pack",
            ))
            .expect("utility science technology should exist");

        assert_eq!(
            unlock_text(&catalog, technology),
            "Low Density Structure, Processing Unit, Flying Robot Frame, Utility Science Pack"
        );
    }

    #[test]
    fn repeatable_technology_formats_next_level_cost_and_effect() {
        let mut sim = factory_sim::Simulation::new_test_world(123);
        let technology_id =
            factory_data::technology_id_by_name(sim.catalog(), "mining_productivity");
        let technology = sim
            .catalog()
            .technology(technology_id)
            .expect("mining productivity should exist")
            .clone();

        assert_eq!(
            next_science_cost_text(&sim, &technology),
            "Automation Science Pack x1, Logistic Science Pack x1, Chemical Science Pack x1, Production Science Pack x1, Utility Science Pack x1, Space Science Pack x1; 500 units"
        );
        assert_eq!(
            unlock_text(sim.catalog(), &technology),
            "+5% mining-drill productivity per level"
        );

        sim.research
            .technology_state_mut(technology_id)
            .expect("technology state should exist")
            .completed_levels = 2;
        assert_eq!(
            next_science_cost_text(&sim, &technology),
            "Automation Science Pack x1, Logistic Science Pack x1, Chemical Science Pack x1, Production Science Pack x1, Utility Science Pack x1, Space Science Pack x1; 1500 units"
        );
    }
}
