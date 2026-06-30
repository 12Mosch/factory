use factory_data::ItemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ItemStack {
    pub item_id: ItemId,
    pub count: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryError {
    UnknownItem,
    InsufficientSpace,
    InsufficientItems,
}
