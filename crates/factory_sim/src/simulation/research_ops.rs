use super::*;

impl ResearchState {
    pub(super) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            technology_names: catalog
                .technologies()
                .iter()
                .map(|technology| technology.name.clone())
                .collect(),
            active: None,
            queue: Vec::new(),
            technologies: catalog
                .technologies()
                .iter()
                .map(|technology| TechnologyResearchState {
                    technology_id: technology.id,
                    completed_levels: 0,
                    progress_units: 0,
                })
                .collect(),
        }
    }

    pub fn is_unlocked(&self, technology_name: &str) -> bool {
        let technology_name = match technology_name {
            "basic-automation" => "automation",
            other => other,
        };

        self.technology_names
            .iter()
            .position(|name| name == technology_name)
            .and_then(|index| self.technologies.get(index))
            .is_some_and(|state| state.completed_levels > 0)
    }
}

pub(super) fn add_research_units_to_state(
    catalog: &PrototypeCatalog,
    research: &mut ResearchState,
    units: u64,
) -> Result<ResearchProgressResult, ResearchError> {
    let technology_id = research.active.ok_or(ResearchError::NoActiveResearch)?;
    let technology = catalog
        .technology(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?;
    let current_level = research
        .technology_state(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?
        .completed_levels;
    let next_level = current_level
        .checked_add(1)
        .ok_or(ResearchError::MaxLevelReached(technology_id))?;
    let required_units = technology
        .required_units_for_level(next_level)
        .ok_or(match technology.level_model {
            factory_data::TechnologyLevelModel::Finite => {
                ResearchError::AlreadyResearched(technology_id)
            }
            factory_data::TechnologyLevelModel::Repeatable { .. } => {
                ResearchError::MaxLevelReached(technology_id)
            }
        })?;
    let state = research
        .technology_state_mut(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?;

    state.progress_units = state
        .progress_units
        .saturating_add(units)
        .min(required_units);

    if state.progress_units >= required_units {
        state.completed_levels = next_level;
        if technology.level_model.is_repeatable() {
            state.progress_units = 0;
        }
        research.active = None;
        promote_next_queued_research_in_state(catalog, research)?;
        Ok(ResearchProgressResult::Completed {
            technology_id,
            completed_level: next_level,
        })
    } else {
        Ok(ResearchProgressResult::InProgress {
            technology_id,
            progress_units: state.progress_units,
            required_units,
        })
    }
}

pub(super) fn promote_next_queued_research_in_state(
    catalog: &PrototypeCatalog,
    research: &mut ResearchState,
) -> Result<(), ResearchError> {
    if research.active.is_some() || research.queue.is_empty() {
        return Ok(());
    }

    let technology_id = research.queue[0];
    can_select_research_in_state(catalog, research, technology_id)?;
    research.queue.remove(0);
    research.active = Some(technology_id);
    Ok(())
}

pub(super) fn can_select_research_in_state(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    technology_id: TechnologyId,
) -> Result<(), ResearchError> {
    if research.active == Some(technology_id) {
        return Err(ResearchError::AlreadyActive(technology_id));
    }
    let technology = catalog
        .technology(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?;
    let state = research
        .technology_state(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?;
    if technology
        .required_units_after(state.completed_levels)
        .is_none()
    {
        return Err(if technology.level_model.is_repeatable() {
            ResearchError::MaxLevelReached(technology_id)
        } else {
            ResearchError::AlreadyResearched(technology_id)
        });
    }

    for prerequisite_id in &technology.prerequisites {
        if research
            .technology_state(*prerequisite_id)
            .is_none_or(|state| state.completed_levels == 0)
        {
            return Err(ResearchError::PrerequisiteLocked {
                technology_id,
                prerequisite_id: *prerequisite_id,
            });
        }
    }

    Ok(())
}

/// Validates a queued-research move without modifying the queue or any other
/// simulation state.
pub(super) fn can_move_queued_research_in_state(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    from_index: usize,
    to_index: usize,
) -> Result<(), ResearchError> {
    let queue = &research.queue;
    let len = queue.len();
    if from_index >= len {
        return Err(ResearchError::InvalidQueueIndex { index: from_index });
    }
    if to_index >= len {
        return Err(ResearchError::InvalidQueueIndex { index: to_index });
    }
    if from_index == to_index {
        return Ok(());
    }

    validate_research_queue_order_in_state(
        catalog,
        research,
        (0..len).map(|index| queued_research_after_move(queue, from_index, to_index, index)),
    )
}

/// Validates a research queue using only the prototype catalog and research
/// state. This keeps queue validation independent from the rest of the world.
pub(super) fn validate_research_queue_order_in_state(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    queue: impl IntoIterator<Item = TechnologyId>,
) -> Result<(), ResearchError> {
    let mut available = research
        .technologies
        .iter()
        .filter(|state| state.completed_levels > 0)
        .map(|state| state.technology_id)
        .collect::<Vec<_>>();
    if let Some(active_id) = research.active {
        let state = research
            .technology_state(active_id)
            .ok_or(ResearchError::MissingTechnology(active_id))?;
        if catalog
            .technology(active_id)
            .and_then(|technology| technology.required_units_after(state.completed_levels))
            .is_none()
        {
            return Err(ResearchError::AlreadyResearched(active_id));
        }
        available.push(active_id);
    }
    let mut seen = Vec::new();

    for technology_id in queue {
        if research.active == Some(technology_id) {
            return Err(ResearchError::AlreadyActive(technology_id));
        }
        if seen.contains(&technology_id) {
            return Err(ResearchError::AlreadyQueued(technology_id));
        }

        let technology = catalog
            .technology(technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))?;
        let state = research
            .technology_state(technology_id)
            .ok_or(ResearchError::MissingTechnology(technology_id))?;
        if technology
            .required_units_after(state.completed_levels)
            .is_none()
        {
            return Err(if technology.level_model.is_repeatable() {
                ResearchError::MaxLevelReached(technology_id)
            } else {
                ResearchError::AlreadyResearched(technology_id)
            });
        }

        for prerequisite_id in &technology.prerequisites {
            if !available.contains(prerequisite_id) {
                return Err(ResearchError::PrerequisiteLocked {
                    technology_id,
                    prerequisite_id: *prerequisite_id,
                });
            }
        }

        seen.push(technology_id);
        available.push(technology_id);
    }

    Ok(())
}

fn queued_research_after_move(
    queue: &[TechnologyId],
    from_index: usize,
    to_index: usize,
    index: usize,
) -> TechnologyId {
    if index == to_index {
        return queue[from_index];
    }

    if from_index < to_index && (from_index..to_index).contains(&index) {
        return queue[index + 1];
    }

    if to_index < from_index && ((to_index + 1)..=from_index).contains(&index) {
        return queue[index - 1];
    }

    queue[index]
}
