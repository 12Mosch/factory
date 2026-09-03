use factory_data::{PrototypeCatalog, TechnologyEffect, TechnologyId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResearchState {
    pub technology_names: Vec<String>,
    pub active: Option<TechnologyId>,
    pub queue: Vec<TechnologyId>,
    pub technologies: Vec<TechnologyResearchState>,
}

impl ResearchState {
    /// Looks up the per-technology state by id, guarding against an id that
    /// does not match the state stored at its index.
    pub fn technology_state(&self, id: TechnologyId) -> Option<&TechnologyResearchState> {
        self.technologies
            .get(id.index())
            .filter(|state| state.technology_id == id)
    }

    pub fn technology_state_mut(
        &mut self,
        id: TechnologyId,
    ) -> Option<&mut TechnologyResearchState> {
        self.technologies
            .get_mut(id.index())
            .filter(|state| state.technology_id == id)
    }

    pub fn bonuses(&self, catalog: &PrototypeCatalog) -> ResearchBonuses {
        let mut bonuses = ResearchBonuses::default();
        for technology in catalog.technologies() {
            let completed_levels = self
                .technology_state(technology.id)
                .map_or(0, |state| state.completed_levels);
            if completed_levels == 0 {
                continue;
            }
            for effect in &technology.effects {
                if let TechnologyEffect::MiningDrillProductivity { bonus_permyriad } = *effect {
                    bonuses.mining_drill_productivity_permyriad = bonuses
                        .mining_drill_productivity_permyriad
                        .saturating_add(u64::from(bonus_permyriad) * u64::from(completed_levels));
                }
            }
        }
        bonuses
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResearchBonuses {
    pub mining_drill_productivity_permyriad: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TechnologyResearchState {
    pub technology_id: TechnologyId,
    pub completed_levels: u32,
    pub progress_units: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResearchError {
    MissingTechnology(TechnologyId),
    AlreadyResearched(TechnologyId),
    MaxLevelReached(TechnologyId),
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
        progress_units: u64,
        required_units: u64,
    },
    Completed {
        technology_id: TechnologyId,
        completed_level: u32,
    },
}
