use bevy::prelude::*;
use factory_data::{PrototypeCatalog, TechnologyEffect, TechnologyId};

use crate::resources::TechnologyWindowState;
use crate::ui::formatting::{format_item_display_name, format_recipe_display_name};

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
        unlocked: sim
            .catalog()
            .technologies
            .iter()
            .map(|technology| sim.is_technology_unlocked(technology.id))
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
        .catalog()
        .technology(technology_id)
        .map(|technology| technology.required_units)
        .unwrap_or(0);
    format!("{progress}/{required}")
}

pub(crate) fn technology_ui_state(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> TechnologyUiState {
    if sim.is_technology_unlocked(technology_id) {
        TechnologyUiState::Researched
    } else if sim.active_research() == Some(technology_id) {
        TechnologyUiState::Researching
    } else if sim.research_queue().contains(&technology_id) {
        TechnologyUiState::Queued
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
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn start_queue_label(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> String {
    if sim.is_technology_unlocked(technology_id) {
        "Researched".to_string()
    } else if sim.active_research() == Some(technology_id) {
        "Researching".to_string()
    } else if sim.research_queue().contains(&technology_id) {
        "Queued".to_string()
    } else if sim.can_enqueue_research(technology_id).is_ok() {
        if sim.active_research().is_some() {
            "Queue Research".to_string()
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
