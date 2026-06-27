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
