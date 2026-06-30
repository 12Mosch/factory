use crate::simulation::*;

pub(in crate::simulation) fn try_consume_fuel(
    catalog: &PrototypeCatalog,
    energy: &mut BurnerEnergy,
) -> Option<ItemId> {
    let mut fuel_stack = energy.fuel_slot?;
    let fuel_value = fuel_value_joules(catalog, fuel_stack.item_id)?;

    let item_id = fuel_stack.item_id;
    fuel_stack.count -= 1;
    energy.fuel_slot = (fuel_stack.count > 0).then_some(fuel_stack);
    energy.energy_remaining_joules += fuel_value as f64;

    Some(item_id)
}
