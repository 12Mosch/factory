use crate::ids::EntityId;
use crate::inventory::ItemStack;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum InserterState {
    WaitingForItem,
    Picking { ticks_left: u32 },
    Holding { item: ItemStack },
    Dropping { ticks_left: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InserterTransferPreview {
    pub pickup_tile: (crate::world::WorldTileCoord, crate::world::WorldTileCoord),
    pub drop_tile: (crate::world::WorldTileCoord, crate::world::WorldTileCoord),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InserterError {
    MissingEntity(EntityId),
    NotInserter(EntityId),
}
