use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn place_stone_furnace(sim: &mut Simulation) -> EntityId {
    let furnace = entity_id_by_name(&sim.world.prototypes, "stone_furnace");
    let (x, y) = first_buildable_rect(&sim.world, 2, 2);
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: furnace,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("stone furnace should be placeable")
}

pub(in crate::simulation::tests) fn place_assembling_machine(sim: &mut Simulation) -> EntityId {
    let assembler = entity_id_by_name(&sim.world.prototypes, "assembling_machine");
    let (x, y) = place_powered_fixture_origin(sim, 3, 3, (3, 1));
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: assembler,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("assembling machine should be placeable")
}

pub(in crate::simulation::tests) fn complete_research_by_name(
    sim: &mut Simulation,
    technology_name: &str,
) {
    let technology_id = technology_id(&sim.world.prototypes, technology_name);
    let required_units = sim.world.prototypes.technologies[technology_id.index()].required_units;

    sim.select_research(technology_id)
        .unwrap_or_else(|_| panic!("{technology_name} should be selectable"));
    sim.add_research_units(required_units)
        .unwrap_or_else(|_| panic!("{technology_name} should complete"));
}

pub(in crate::simulation::tests) fn add_furnace_input_and_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    input_item: ItemId,
    fuel_item: ItemId,
) {
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: input_item,
        count: 1,
    });
    sim.player_inventory.slots[1] = Some(ItemStack {
        item_id: fuel_item,
        count: 1,
    });
    crate::entity_transfer::player_slot_to_furnace_input(sim, entity_id, 0)
        .expect("input should transfer to furnace");
    crate::entity_transfer::player_slot_to_furnace_fuel(sim, entity_id, 1)
        .expect("fuel should transfer to furnace");
}

pub(in crate::simulation::tests) fn place_lab(sim: &mut Simulation) -> EntityId {
    let lab = entity_id_by_name(&sim.world.prototypes, "lab");
    let (x, y) = place_powered_fixture_origin(sim, 3, 3, (3, 1));

    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: lab,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("lab should be placeable")
}

pub(in crate::simulation::tests) fn add_assembler_gear_job(
    sim: &mut Simulation,
    assembler_id: EntityId,
) {
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");

    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("gear recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 2,
    });
    crate::entity_transfer::player_slot_to_assembler_input(sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");
}

pub(in crate::simulation::tests) fn run_same_assembler_actions(sim: &mut Simulation) {
    let assembler_id = place_assembling_machine(sim);
    let recipe = recipe_id(&sim.world.prototypes, "iron_gear_wheel");
    let iron_plate = item_id(&sim.world.prototypes, "iron_plate");
    sim.select_assembler_recipe(assembler_id, recipe)
        .expect("crafting recipe should be accepted by assembler");
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: iron_plate,
        count: 4,
    });
    crate::entity_transfer::player_slot_to_assembler_input(sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");
    for _ in 0..125 {
        sim.tick();
    }
}
