use super::super::*;
use super::ids::*;

pub(super) fn validate_research_state(sim: &Simulation) -> Result<(), SimValidationError> {
    let technology_names = sim
        .world
        .prototypes
        .technologies()
        .iter()
        .map(|technology| technology.name.clone())
        .collect::<Vec<_>>();
    if sim.research.technology_names != technology_names {
        return Err(SimValidationError::InvalidResearchTechnologyNames);
    }

    if sim.research.technologies.len() != sim.world.prototypes.technologies().len() {
        return Err(SimValidationError::InvalidResearchTechnologyNames);
    }

    for (index, technology) in sim.world.prototypes.technologies().iter().enumerate() {
        let state = sim.research.technology_state(technology.id).ok_or(
            SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            },
        )?;

        if state.technology_id.index() != index {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: state.technology_id,
            });
        }

        if !technology.level_model.is_repeatable() && state.completed_levels > 1 {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            });
        }

        let required_units = match state.completed_levels.checked_add(1) {
            Some(next_level) => technology.required_units_for_level(next_level),
            None => None,
        };
        let valid_progress = if let Some(required_units) = required_units {
            state.progress_units < required_units
        } else {
            if technology.level_model.is_repeatable() {
                state.completed_levels == u32::MAX && state.progress_units == 0
            } else {
                state.completed_levels == 1 && state.progress_units == technology.required_units
            }
        };
        if !valid_progress {
            return Err(SimValidationError::InvalidResearchProgress {
                technology_id: technology.id,
                progress_units: state.progress_units,
                required_units: required_units.unwrap_or(technology.required_units),
            });
        }
        if state.completed_levels > 0
            && technology
                .prerequisites
                .iter()
                .any(|prerequisite_id| !technology_researched(&sim.research, *prerequisite_id))
        {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            });
        }
    }

    for state in &sim.research.technologies {
        if sim
            .world
            .prototypes
            .technology(state.technology_id)
            .is_none()
        {
            return Err(SimValidationError::InvalidResearchTechnology {
                technology_id: state.technology_id,
            });
        }
    }

    if let Some(technology_id) = sim.research.active {
        let technology = sim
            .world
            .prototypes
            .technology(technology_id)
            .ok_or(SimValidationError::InvalidActiveResearch { technology_id })?;
        let state = sim
            .research
            .technology_state(technology_id)
            .ok_or(SimValidationError::InvalidActiveResearch { technology_id })?;
        let has_next_level = state
            .completed_levels
            .checked_add(1)
            .and_then(|level| technology.required_units_for_level(level))
            .is_some();
        if !has_next_level {
            return Err(SimValidationError::InvalidActiveResearch { technology_id });
        }
        for prerequisite_id in &technology.prerequisites {
            if !technology_researched(&sim.research, *prerequisite_id) {
                return Err(SimValidationError::InvalidActiveResearch { technology_id });
            }
        }
    }

    let mut available = sim
        .research
        .technologies
        .iter()
        .filter(|state| state.completed_levels > 0)
        .map(|state| state.technology_id)
        .collect::<BTreeSet<_>>();
    if let Some(technology_id) = sim.research.active {
        available.insert(technology_id);
    }
    let mut queued = BTreeSet::new();
    for technology_id in &sim.research.queue {
        let technology = sim.world.prototypes.technology(*technology_id).ok_or(
            SimValidationError::InvalidQueuedResearch {
                technology_id: *technology_id,
            },
        )?;
        let state = sim.research.technology_state(*technology_id).ok_or(
            SimValidationError::InvalidQueuedResearch {
                technology_id: *technology_id,
            },
        )?;

        let has_next_level = state
            .completed_levels
            .checked_add(1)
            .and_then(|level| technology.required_units_for_level(level))
            .is_some();
        if !has_next_level
            || Some(*technology_id) == sim.research.active
            || !queued.insert(*technology_id)
        {
            return Err(SimValidationError::InvalidQueuedResearch {
                technology_id: *technology_id,
            });
        }
        if technology
            .prerequisites
            .iter()
            .any(|prerequisite_id| !available.contains(prerequisite_id))
        {
            return Err(SimValidationError::InvalidQueuedResearch {
                technology_id: *technology_id,
            });
        }

        available.insert(*technology_id);
    }

    Ok(())
}
