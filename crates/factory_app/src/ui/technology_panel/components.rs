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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TechnologyPanelSnapshot {
    pub(crate) selected: Option<TechnologyId>,
    pub(crate) active: Option<TechnologyId>,
    pub(crate) queue: Vec<TechnologyId>,
    pub(crate) progress_units: Vec<u32>,
    pub(crate) unlocked: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TechnologyUiState {
    Researched,
    Researching,
    Queued,
    Available,
    Locked,
}
