use super::super::super::*;

pub(in crate::simulation::tests) fn total_item_count_in_sim(
    sim: &Simulation,
    item_id: ItemId,
) -> u32 {
    sim.player_inventory.count(item_id)
        + sim
            .entities
            .entity_inventories
            .values()
            .map(|inventory| inventory.count(item_id))
            .sum::<u32>()
        + sim
            .entities
            .labs
            .values()
            .map(|lab| lab.inventory.count(item_id))
            .sum::<u32>()
        + sim
            .entities
            .furnaces
            .values()
            .map(|furnace| {
                count_slot_item(furnace.input_slot, item_id)
                    + count_slot_item(furnace.energy.fuel_slot, item_id)
                    + count_slot_item(furnace.output_slot, item_id)
            })
            .sum::<u32>()
        + sim
            .entities
            .burner_mining_drills
            .values()
            .map(|drill| {
                count_slot_item(drill.energy.fuel_slot, item_id)
                    + count_slot_item(drill.output_slot, item_id)
            })
            .sum::<u32>()
        + sim
            .entities
            .assembling_machines
            .values()
            .map(|assembler| {
                assembler.input_inventory.count(item_id) + assembler.output_inventory.count(item_id)
            })
            .sum::<u32>()
        + total_belt_count_for_item(sim, item_id)
        + sim
            .entities
            .inserters
            .values()
            .map(|state| match state {
                InserterState::Holding { item } if item.item_id() == item_id => {
                    u32::from(item.count())
                }
                _ => 0,
            })
            .sum::<u32>()
}

pub(in crate::simulation::tests) fn total_belt_count_for_item(
    sim: &Simulation,
    item_id: ItemId,
) -> u32 {
    let belt_count = sim
        .entities
        .transport_belts
        .values()
        .map(|segment| {
            segment
                .lanes
                .iter()
                .flat_map(|lane| lane.items.iter())
                .filter(|item| item.item_id == item_id)
                .count() as u32
        })
        .sum::<u32>();
    let splitter_count = sim
        .entities
        .splitters
        .values()
        .map(|state| {
            state
                .input_lanes
                .iter()
                .flat_map(|input_lanes| input_lanes.iter())
                .flat_map(|lane| lane.items.iter())
                .filter(|item| item.item_id == item_id)
                .count() as u32
        })
        .sum::<u32>();

    belt_count + splitter_count
}

pub(in crate::simulation::tests) fn count_slot_item(
    slot: Option<ItemStack>,
    item_id: ItemId,
) -> u32 {
    match slot {
        Some(stack) if stack.item_id() == item_id => u32::from(stack.count()),
        _ => 0,
    }
}
