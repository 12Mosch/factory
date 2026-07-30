use crate::ids::EntityId;
use crate::rolling_stock::RollingStockId;
use factory_data::{FluidConnectionSide, FluidId};
use serde::{Deserialize, Serialize};

/// What holds a fluid box.
///
/// Almost every fluid box belongs to a placed entity, and did so exclusively
/// until wagons had to join a network. A fluid wagon is not a placed entity —
/// it is not on the tile grid at all — so the network cannot name its box by
/// [`EntityId`], and inventing a synthetic entity id for it would put a
/// non-entity into every map keyed by one.
///
/// Ordering matters: it is what the network builder picks a component's
/// canonical box from, so entities sort before stock and a network's identity
/// does not move about as trains come and go.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum FluidBoxOwner {
    Entity(EntityId),
    RollingStock(RollingStockId),
}

impl FluidBoxOwner {
    pub const fn entity_id(self) -> Option<EntityId> {
        match self {
            Self::Entity(entity_id) => Some(entity_id),
            Self::RollingStock(_) => None,
        }
    }

    pub const fn rolling_stock_id(self) -> Option<RollingStockId> {
        match self {
            Self::RollingStock(stock_id) => Some(stock_id),
            Self::Entity(_) => None,
        }
    }
}

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
    pub owner: FluidBoxOwner,
    pub box_index: usize,
    pub capacity_milliunits: u64,
    pub amount_milliunits: u64,
    pub fluid_id: Option<FluidId>,
    pub filter: Option<FluidId>,
}
