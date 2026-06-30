use factory_data::TechnologyId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResearchState {
    pub technology_names: Vec<String>,
    pub active: Option<TechnologyId>,
    pub queue: Vec<TechnologyId>,
    pub technologies: Vec<TechnologyResearchState>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TechnologyResearchState {
    pub technology_id: TechnologyId,
    pub progress_units: u32,
    pub unlocked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchError {
    MissingTechnology(TechnologyId),
    AlreadyResearched(TechnologyId),
    AlreadyActive(TechnologyId),
    AlreadyQueued(TechnologyId),
    PrerequisiteLocked {
        technology_id: TechnologyId,
        prerequisite_id: TechnologyId,
    },
    InvalidQueueIndex {
        index: usize,
    },
    NoActiveResearch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchProgressResult {
    InProgress {
        technology_id: TechnologyId,
        progress_units: u32,
        required_units: u32,
    },
    Completed {
        technology_id: TechnologyId,
    },
}
