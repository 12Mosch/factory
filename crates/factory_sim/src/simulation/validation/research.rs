use super::super::*;
use super::ids::*;

pub(super) fn validate_research_state(sim: &Simulation) -> Result<(), SimValidationError> {
    let technology_names = sim
        .world
        .prototypes
        .technologies
        .iter()
        .map(|technology| technology.name.clone())
        .collect::<Vec<_>>();
    if sim.research.technology_names != technology_names {
        return Err(SimValidationError::InvalidResearchTechnologyNames);
    }

    for technology in &sim.world.prototypes.technologies {
        let state = sim.research.technology_state(technology.id).ok_or(
            SimValidationError::InvalidResearchTechnology {
                technology_id: technology.id,
            },
        )?;

        if state.progress_units > technology.required_units {
            return Err(SimValidationError::InvalidResearchProgress {
                technology_id: technology.id,
                progress_units: state.progress_units,
                required_units: technology.required_units,
            });
        }
        if state.unlocked
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
        if state.unlocked {
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
        .filter(|state| state.unlocked)
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

        if state.unlocked
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
