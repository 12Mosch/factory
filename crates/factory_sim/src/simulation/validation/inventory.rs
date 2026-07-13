use super::super::*;

pub(in crate::simulation) fn validate_inventory(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
) -> Result<(), SimValidationError> {
    for slot in inventory.slots() {
        validate_item_slot(catalog, *slot)?;
    }

    Ok(())
}

pub(super) fn validate_item_slot(
    catalog: &PrototypeCatalog,
    slot: ItemSlot,
) -> Result<(), SimValidationError> {
    slot.validate(catalog).map_err(map_inventory_error)
}

pub(super) fn validate_item_stack(
    catalog: &PrototypeCatalog,
    stack: ItemStack,
) -> Result<(), SimValidationError> {
    if stack.count() == 0 {
        return Err(SimValidationError::EmptyItemStack(stack.item_id()));
    }

    let stack_size = item_stack_size(catalog, stack.item_id())
        .ok_or(SimValidationError::UnknownItem(stack.item_id()))?;
    if stack.count() > stack_size {
        return Err(SimValidationError::StackExceedsLimit {
            item_id: stack.item_id(),
            count: stack.count(),
            stack_size,
        });
    }

    Ok(())
}

fn map_inventory_error(error: InventoryError) -> SimValidationError {
    match error {
        InventoryError::UnknownItem(item_id) => SimValidationError::UnknownItem(item_id),
        InventoryError::EmptyItemStack(item_id) => SimValidationError::EmptyItemStack(item_id),
        InventoryError::StackExceedsLimit {
            item_id,
            count,
            stack_size,
        } => SimValidationError::StackExceedsLimit {
            item_id,
            count,
            stack_size,
        },
        InventoryError::InvalidSlot { .. }
        | InventoryError::EmptySlot { .. }
        | InventoryError::InsufficientSpace
        | InventoryError::InsufficientItems => {
            unreachable!("validating one item slot cannot report inventory operation errors")
        }
    }
}
