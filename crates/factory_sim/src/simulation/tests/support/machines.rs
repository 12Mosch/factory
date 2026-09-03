use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn place_stone_furnace(sim: &mut Simulation) -> EntityId {
    place_named_furnace(sim, "stone_furnace")
}

pub(in crate::simulation::tests) fn place_named_furnace(
    sim: &mut Simulation,
    furnace_name: &str,
) -> EntityId {
    let furnace = entity_id_by_name(&sim.world.prototypes, furnace_name);
    let prototype = &sim.world.prototypes.entities()[furnace.index()];
    let (width, height) = (prototype.size.x, prototype.size.y);
    let (x, y) = first_buildable_rect(&sim.world, width, height);
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: furnace,
            x,
            y,
            direction: Direction::North,
        },
    )
    .unwrap_or_else(|_| panic!("{furnace_name} should be placeable"))
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
    let required_units = sim.world.prototypes.technologies()[technology_id.index()].required_units;

    sim.select_research(technology_id)
        .unwrap_or_else(|_| panic!("{technology_name} should be selectable"));
    sim.add_research_units(required_units)
        .unwrap_or_else(|_| panic!("{technology_name} should complete"));
}

/// Researches `technology_name` and everything it depends on, so whatever it
/// unlocks is reached through the same technology gate a player passes rather
/// than around it. Prerequisites are followed rather than listed: the chain is
/// a property of the catalog, and a hand-written list is one catalog edit away
/// from being wrong.
pub(in crate::simulation::tests) fn unlock_with_prerequisites(
    sim: &mut Simulation,
    technology_name: &str,
) {
    if sim.research.is_unlocked(technology_name) {
        return;
    }
    let technology_id = technology_id(&sim.world.prototypes, technology_name);
    let prerequisites = sim.world.prototypes.technologies()[technology_id.index()]
        .prerequisites
        .clone();
    for prerequisite in prerequisites {
        let name = sim.world.prototypes.technologies()[prerequisite.index()]
            .name
            .clone();
        unlock_with_prerequisites(sim, &name);
    }
    complete_research_by_name(sim, technology_name);
}

/// A chemical plant standing on powered ground, ready for a recipe.
pub(in crate::simulation::tests) fn place_powered_chemical_plant(sim: &mut Simulation) -> EntityId {
    let chemical_plant = entity_id_by_name(&sim.world.prototypes, "chemical_plant");
    let (x, y) = place_powered_fixture_origin(sim, 3, 3, (3, 1));
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: chemical_plant,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("chemical plant should be placeable")
}

/// A researched rocket silo standing on powered ground.
///
/// The pole goes on the silo's west edge rather than beside a corner: at nine
/// tiles across, a pole placed the way the smaller fixtures place theirs would
/// be out of wire reach of the fixture's source pole.
pub(in crate::simulation::tests) fn place_powered_rocket_silo(sim: &mut Simulation) -> EntityId {
    unlock_with_prerequisites(sim, "rocket_silo");
    let rocket_silo = entity_id_by_name(&sim.world.prototypes, "rocket_silo");
    let (x, y) = place_powered_fixture_origin(sim, 9, 9, (-1, 4));
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id: rocket_silo,
            x,
            y,
            direction: Direction::North,
        },
    )
    .expect("rocket silo should be placeable")
}

/// Fills the silo's ingredient slots with `parts` cycles' worth of every
/// ingredient its recipe asks for, so a test can run it without restocking.
pub(in crate::simulation::tests) fn stock_rocket_silo(
    sim: &mut Simulation,
    entity_id: EntityId,
    parts: u16,
) {
    let ingredients = sim
        .rocket_silo_recipe()
        .expect("the rocket part recipe should be unlocked")
        .ingredients
        .clone();

    sim.player_inventory = Inventory::player();
    for (slot_index, ingredient) in ingredients.iter().enumerate() {
        set_inventory_slot(
            &mut sim.player_inventory,
            slot_index,
            ingredient.item,
            ingredient.amount * parts,
        );
        crate::entity_transfer::player_slot_to_rocket_silo_input(sim, entity_id, slot_index)
            .expect("silo ingredients should transfer");
    }
}

pub(in crate::simulation::tests) fn add_furnace_input_and_fuel(
    sim: &mut Simulation,
    entity_id: EntityId,
    input_item: ItemId,
    fuel_item: ItemId,
) {
    sim.player_inventory = Inventory::player();
    set_inventory_slot(&mut sim.player_inventory, 0, input_item, 1);
    set_inventory_slot(&mut sim.player_inventory, 1, fuel_item, 1);
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
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 2);
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
    set_inventory_slot(&mut sim.player_inventory, 0, iron_plate, 4);
    crate::entity_transfer::player_slot_to_assembler_input(sim, assembler_id, 0)
        .expect("assembler should accept gear ingredients");
    for _ in 0..125 {
        sim.tick();
    }
}
