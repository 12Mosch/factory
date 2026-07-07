use super::*;

impl ResearchState {
    pub(super) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            technology_names: catalog
                .technologies
                .iter()
                .map(|technology| technology.name.clone())
                .collect(),
            active: None,
            queue: Vec::new(),
            technologies: catalog
                .technologies
                .iter()
                .map(|technology| TechnologyResearchState {
                    technology_id: technology.id,
                    progress_units: 0,
                    unlocked: false,
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
            .is_some_and(|state| state.unlocked)
    }
}

pub(super) fn add_research_units_to_state(
    catalog: &PrototypeCatalog,
    research: &mut ResearchState,
    units: u32,
) -> Result<ResearchProgressResult, ResearchError> {
    let technology_id = research.active.ok_or(ResearchError::NoActiveResearch)?;
    let technology = catalog
        .technology(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?;
    let state = research
        .technology_state_mut(technology_id)
        .ok_or(ResearchError::MissingTechnology(technology_id))?;

    state.progress_units = state
        .progress_units
        .saturating_add(units)
        .min(technology.required_units);

    if state.progress_units >= technology.required_units {
        state.unlocked = true;
        research.active = None;
        promote_next_queued_research_in_state(catalog, research)?;
        Ok(ResearchProgressResult::Completed { technology_id })
    } else {
        Ok(ResearchProgressResult::InProgress {
            technology_id,
            progress_units: state.progress_units,
            required_units: technology.required_units,
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
    if state.unlocked {
        return Err(ResearchError::AlreadyResearched(technology_id));
    }

    for prerequisite_id in &technology.prerequisites {
        if !research
            .technology_state(*prerequisite_id)
            .is_some_and(|state| state.unlocked)
        {
            return Err(ResearchError::PrerequisiteLocked {
                technology_id,
                prerequisite_id: *prerequisite_id,
            });
        }
    }

    Ok(())
}
