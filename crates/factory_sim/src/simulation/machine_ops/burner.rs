use crate::simulation::*;

pub(in crate::simulation) fn try_consume_fuel(
    catalog: &PrototypeCatalog,
    energy: &mut BurnerEnergy,
) -> Option<ItemId> {
    let fuel_stack = energy.fuel_slot?;
    let fuel_value = fuel_value_joules(catalog, fuel_stack.item_id())?;

    let item_id = fuel_stack.item_id();
    remove_from_single_slot(&mut energy.fuel_slot, item_id, 1)
        .expect("the available fuel stack contains one item");
    energy.energy_remaining_joules += fuel_value as f64;

    Some(item_id)
}
