use bevy::prelude::*;
use factory_data::TechnologyId;

#[derive(Component)]
pub struct TechnologySelectButton {
    pub technology_id: TechnologyId,
}

#[derive(Component)]
pub struct TechnologyStartQueueButton;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechnologyQueueAction {
    Remove,
    MoveUp,
    MoveDown,
}

#[derive(Component)]
pub struct TechnologyQueueButton {
    pub index: usize,
    pub action: TechnologyQueueAction,
}

#[derive(Component)]
pub(crate) struct TechnologyPanelContentRoot;

#[derive(Component)]
pub(crate) struct TechnologyListRoot;

#[derive(Component)]
pub(crate) struct TechnologyDetailRoot;

#[derive(Component)]
pub(crate) struct ActiveResearchText;

#[derive(Component)]
pub(crate) struct ResearchQueueText;

#[derive(Component)]
pub(crate) struct TechnologyStatusText(pub(crate) TechnologyId);

#[derive(Component)]
pub(crate) struct TechnologyDetailText(pub(crate) TechnologyDetailField);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TechnologyDetailField {
    Name,
    Level,
    Progress,
    Prerequisites,
    Cost,
    Effects,
    Start,
}

#[derive(Component)]
pub(crate) struct TechnologyProgressFill;

#[derive(Component)]
pub(crate) struct TechnologyQueueRoot;

#[derive(Component)]
pub(crate) struct TechnologyQueueEmpty;

#[derive(Component)]
pub(crate) struct TechnologyQueueTitle;

#[derive(Component)]
pub(crate) struct TechnologyQueueRow(pub(crate) TechnologyId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TechnologyPanelSnapshot {
    pub(crate) selected: Option<TechnologyId>,
    pub(crate) replacement_revision: u64,
    pub(crate) research_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TechnologyUiState {
    Researched,
    Researching,
    Queued,
    Available,
    Locked,
}
