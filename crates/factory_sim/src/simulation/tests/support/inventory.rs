use super::super::super::*;
use std::sync::OnceLock;

pub(in crate::simulation::tests) fn set_inventory_slot(
    inventory: &mut Inventory,
    slot_index: usize,
    item_id: ItemId,
    count: u16,
) {
    static CATALOG: OnceLock<PrototypeCatalog> = OnceLock::new();
    let catalog = CATALOG
        .get_or_init(|| PrototypeCatalog::load_base().expect("base prototype catalog should load"));
    let mut slots = inventory.slots().to_vec();
    let slot = slots
        .get_mut(slot_index)
        .expect("test inventory slot index should be valid");
    *slot = Some(
        ItemStack::new(catalog, item_id, count)
            .expect("test inventory stack should satisfy catalog invariants"),
    );
    *inventory = Inventory::from_slots(catalog, slots)
        .expect("test inventory layout should satisfy catalog invariants");
}
