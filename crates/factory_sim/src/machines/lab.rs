use super::MachineModuleState;
use crate::ids::EntityId;
use crate::inventory::Inventory;
use factory_data::TechnologyId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LabState {
    pub modules: MachineModuleState,
    pub inventory: Inventory,
    pub active_technology: Option<TechnologyId>,
    pub progress_ticks: u32,
    pub required_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabError {
    MissingEntity(EntityId),
    NotLab(EntityId),
}
