use super::super::*;

pub(super) fn validate_inventory(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
) -> Result<(), SimValidationError> {
    for stack in inventory.slots.iter().flatten() {
        validate_item_stack(catalog, *stack)?;
    }

    Ok(())
}

pub(super) fn validate_item_stack(
    catalog: &PrototypeCatalog,
    stack: ItemStack,
) -> Result<(), SimValidationError> {
    if stack.count == 0 {
        return Err(SimValidationError::EmptyItemStack(stack.item_id));
    }

    let stack_size = item_stack_size(catalog, stack.item_id)
        .ok_or(SimValidationError::UnknownItem(stack.item_id))?;
    if stack.count > stack_size {
        return Err(SimValidationError::StackExceedsLimit {
            item_id: stack.item_id,
            count: stack.count,
            stack_size,
        });
    }

    Ok(())
}
