use crate::simulation::*;

pub(in crate::simulation) fn burner_fuel_slot_can_accept(
    catalog: &PrototypeCatalog,
    fuel_slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    if fuel_value_joules(catalog, stack.item_id).is_none() {
        return false;
    }

    let Some(stack_size) = item_stack_size(catalog, stack.item_id) else {
        return false;
    };

    match fuel_slot {
        None => stack.count <= stack_size,
        Some(existing) if existing.item_id == stack.item_id => {
            u32::from(existing.count) + u32::from(stack.count) <= u32::from(stack_size)
        }
        Some(_) => false,
    }
}

pub(in crate::simulation) fn output_slot_can_accept(
    catalog: &PrototypeCatalog,
    output_slot: Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> bool {
    let Some(stack_size) = item_stack_size(catalog, item_id) else {
        return false;
    };

    match output_slot {
        None => count <= stack_size,
        Some(existing) if existing.item_id == item_id => {
            u32::from(existing.count) + u32::from(count) <= u32::from(stack_size)
        }
        Some(_) => false,
    }
}

pub(in crate::simulation) fn insert_into_single_slot(
    slot: &mut Option<ItemStack>,
    stack: ItemStack,
) {
    match slot {
        Some(existing) => existing.count += stack.count,
        None => *slot = Some(stack),
    }
}

pub(in crate::simulation) fn insert_output_item(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) {
    insert_into_single_slot(slot, ItemStack { item_id, count });
}

pub(in crate::simulation) fn remove_from_single_slot(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> Result<(), InventoryError> {
    let Some(mut stack) = *slot else {
        return Err(InventoryError::InsufficientItems);
    };
    if stack.item_id != item_id || stack.count < count {
        return Err(InventoryError::InsufficientItems);
    }

    stack.count -= count;
    *slot = (stack.count > 0).then_some(stack);
    Ok(())
}
