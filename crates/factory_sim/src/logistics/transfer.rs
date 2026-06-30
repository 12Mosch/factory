use crate::ids::EntityId;
use factory_data::ItemId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerError {
    MissingEntity(EntityId),
    NotContainer(EntityId),
    InvalidItem(ItemId),
    InvalidSlot { slot_index: usize },
    EmptySlot { slot_index: usize },
    InsufficientSpace,
    UnknownItem,
}
