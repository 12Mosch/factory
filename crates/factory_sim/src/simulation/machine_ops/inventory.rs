use crate::simulation::*;

pub(in crate::simulation) fn burner_fuel_slot_can_accept(
    catalog: &PrototypeCatalog,
    fuel_slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    burner_fuel_accepts_item(catalog, stack.item_id())
        && crate::inventory::single_slot_can_accept(catalog, fuel_slot, stack)
}

pub(in crate::simulation) fn burner_fuel_accepts_item(
    catalog: &PrototypeCatalog,
    item_id: ItemId,
) -> bool {
    fuel_value_joules(catalog, item_id).is_some()
}

pub(in crate::simulation) fn output_slot_can_accept(
    catalog: &PrototypeCatalog,
    output_slot: Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> bool {
    ItemStack::new(catalog, item_id, count)
        .is_ok_and(|stack| crate::inventory::single_slot_can_accept(catalog, output_slot, stack))
}

pub(in crate::simulation) fn insert_into_single_slot(
    catalog: &PrototypeCatalog,
    slot: &mut Option<ItemStack>,
    stack: ItemStack,
) -> Result<(), InventoryError> {
    crate::inventory::insert_into_single_slot(catalog, slot, stack)
}

pub(in crate::simulation) fn insert_output_item(
    catalog: &PrototypeCatalog,
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> Result<(), InventoryError> {
    crate::inventory::insert_item_into_single_slot(catalog, slot, item_id, count)
}

pub(in crate::simulation) fn remove_from_single_slot(
    slot: &mut Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> Result<(), InventoryError> {
    crate::inventory::remove_from_single_slot(slot, item_id, count)
}
