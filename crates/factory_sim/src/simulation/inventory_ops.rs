use super::*;

impl From<InventoryError> for ContainerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem(_) => Self::UnknownItem,
            InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
            InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("container transfers remove a known slot stack")
            }
            InventoryError::EmptyItemStack(_) | InventoryError::StackExceedsLimit { .. } => {
                unreachable!("inventory operations only create validated stacks")
            }
        }
    }
}

impl From<InventoryError> for BurnerDrillError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem(_) => Self::UnknownItem,
            InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
            InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("burner drill transfers remove a known slot stack")
            }
            InventoryError::EmptyItemStack(_) | InventoryError::StackExceedsLimit { .. } => {
                unreachable!("inventory operations only create validated stacks")
            }
        }
    }
}

impl From<InventoryError> for FurnaceError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem(_) => Self::UnknownItem,
            InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
            InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("furnace transfers remove a known slot stack")
            }
            InventoryError::EmptyItemStack(_) | InventoryError::StackExceedsLimit { .. } => {
                unreachable!("inventory operations only create validated stacks")
            }
        }
    }
}

impl From<InventoryError> for BoilerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem(_) => Self::UnknownItem,
            InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
            InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("boiler transfers remove a known slot stack")
            }
            InventoryError::EmptyItemStack(_) | InventoryError::StackExceedsLimit { .. } => {
                unreachable!("inventory operations only create validated stacks")
            }
        }
    }
}

impl From<InventoryError> for AssemblerError {
    fn from(error: InventoryError) -> Self {
        match error {
            InventoryError::UnknownItem(_) => Self::UnknownItem,
            InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
            InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
            InventoryError::InsufficientSpace => Self::InsufficientSpace,
            InventoryError::InsufficientItems => {
                unreachable!("assembler transfers remove a known slot stack")
            }
            InventoryError::EmptyItemStack(_) | InventoryError::StackExceedsLimit { .. } => {
                unreachable!("inventory operations only create validated stacks")
            }
        }
    }
}

pub(super) fn stack_in_slot(
    inventory: &Inventory,
    slot_index: usize,
) -> Result<ItemStack, ContainerError> {
    inventory
        .slots()
        .get(slot_index)
        .ok_or(ContainerError::InvalidSlot { slot_index })?
        .ok_or(ContainerError::EmptySlot { slot_index })
}

pub(super) fn ensure_inventory_can_accept(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
    stack: ItemStack,
) -> Result<(), ContainerError> {
    if inventory.can_insert(catalog, stack.item_id(), stack.count()) {
        Ok(())
    } else if item_stack_size(catalog, stack.item_id()).is_none() {
        Err(ContainerError::UnknownItem)
    } else {
        Err(ContainerError::InsufficientSpace)
    }
}
