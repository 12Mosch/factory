use factory_data::ItemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceCell {
    pub resource_item: ItemId,
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceTileChange {
    pub revision: u64,
    pub x: i32,
    pub y: i32,
    pub resource: Option<ResourceCell>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MinedResource {
    pub resource_item: ItemId,
    pub amount: u32,
}
