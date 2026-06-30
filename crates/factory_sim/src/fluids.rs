use crate::ids::EntityId;
use factory_data::{FluidConnectionSide, FluidId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FluidConnectionPreviewState {
    Open,
    Compatible,
    Incompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FluidConnectionPreview {
    pub tile: (i32, i32),
    pub side: FluidConnectionSide,
    pub state: FluidConnectionPreviewState,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidBoxState {
    pub fluid_id: Option<FluidId>,
    pub amount_milliunits: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidNetworkSnapshot {
    pub network_id: u32,
    pub fluid_id: Option<FluidId>,
    pub total_milliunits: u64,
    pub capacity_milliunits: u64,
    pub box_count: usize,
    pub blocked: bool,
    pub boxes: Vec<FluidNetworkBoxSnapshot>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct FluidNetworkBoxSnapshot {
    pub entity_id: EntityId,
    pub box_index: usize,
    pub capacity_milliunits: u64,
    pub amount_milliunits: u64,
    pub fluid_id: Option<FluidId>,
    pub filter: Option<FluidId>,
}
