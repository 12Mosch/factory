use super::*;

macro_rules! impl_inventory_error_conversion {
    ($target:ty, $transfer_context:literal) => {
        impl From<InventoryError> for $target {
            fn from(error: InventoryError) -> Self {
                match error {
                    InventoryError::UnknownItem(_) => Self::UnknownItem,
                    InventoryError::InvalidSlot { slot_index } => Self::InvalidSlot { slot_index },
                    InventoryError::EmptySlot { slot_index } => Self::EmptySlot { slot_index },
                    InventoryError::InsufficientSpace => Self::InsufficientSpace,
                    InventoryError::InsufficientItems => unreachable!(concat!(
                        $transfer_context,
                        " transfers remove a known slot stack"
                    )),
                    InventoryError::EmptyItemStack(_)
                    | InventoryError::StackExceedsLimit { .. } => {
                        unreachable!("inventory operations only create validated stacks")
                    }
                }
            }
        }
    };
}

impl_inventory_error_conversion!(ContainerError, "container");
impl_inventory_error_conversion!(BurnerDrillError, "burner drill");
impl_inventory_error_conversion!(FurnaceError, "furnace");
impl_inventory_error_conversion!(BoilerError, "boiler");
impl_inventory_error_conversion!(AssemblerError, "assembler");

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
