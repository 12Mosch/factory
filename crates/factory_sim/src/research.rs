use factory_data::{PrototypeCatalog, TechnologyEffect, TechnologyId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResearchState {
    pub technology_names: Vec<String>,
    pub active: Option<TechnologyId>,
    pub queue: Vec<TechnologyId>,
    pub technologies: Vec<TechnologyResearchState>,
}

/// Runtime-only invalidation keys for consumers of research state.
///
/// Progress advances frequently while queue and completion state change much
/// less often, so presentation code can avoid treating every science unit as
/// a change to the entire technology graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResearchRevisions {
    pub(crate) any: u64,
    pub(crate) progress: u64,
    pub(crate) queue: u64,
    pub(crate) unlock: u64,
}

impl ResearchRevisions {
    /// Records a change to active research or the pending queue.
    pub(crate) fn bump_queue(&mut self) {
        self.any = self.any.wrapping_add(1);
        self.queue = self.queue.wrapping_add(1);
    }

    /// Records science progress and, on completion, its queue and unlock effects.
    pub(crate) fn bump_progress(&mut self, completed: bool) {
        self.any = self.any.wrapping_add(1);
        self.progress = self.progress.wrapping_add(1);
        if completed {
            // Completion clears/promotes active research and may consume the
            // first queued entry in addition to changing unlock-dependent UI.
            self.queue = self.queue.wrapping_add(1);
            self.unlock = self.unlock.wrapping_add(1);
        }
    }
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
