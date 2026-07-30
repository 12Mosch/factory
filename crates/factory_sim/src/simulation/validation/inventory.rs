use super::super::*;

pub(in crate::simulation) fn validate_inventory(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
) -> Result<(), SimValidationError> {
    for slot in inventory.slots() {
        validate_item_slot(catalog, *slot)?;
    }

    // Filters are either absent for the whole inventory or one per slot, they
    // name items the catalog knows, and none of them contradicts what its slot
    // is holding. The insert path treats all three as facts: it reads a filter
    // by slot index and skips the slot when the filter disagrees, so a save
    // that broke any of them would silently make room disappear.
    let filters = inventory.filters();
    if !filters.is_empty() && filters.len() != inventory.slots().len() {
        return Err(SimValidationError::InvalidInventoryFilters);
    }
    for (slot_index, filter) in filters.iter().enumerate() {
        let Some(filter) = filter else {
            continue;
        };
        if catalog.item(*filter).is_none() {
            return Err(SimValidationError::UnknownItem(*filter));
        }
        if inventory
            .slot(slot_index)
            .is_some_and(|stack| stack.item_id() != *filter)
        {
            return Err(SimValidationError::InvalidInventoryFilters);
        }
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
        | InventoryError::InsufficientItems
        | InventoryError::FilterMismatch { .. } => {
            unreachable!("validating one item slot cannot report inventory operation errors")
        }
    }
}
