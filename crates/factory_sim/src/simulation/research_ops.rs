use super::*;

impl ResearchState {
    pub(super) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            active: None,
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
}
