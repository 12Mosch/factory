use super::common::{
    complete_research_by_name, entity_id_by_name, item_id_by_name, place_powered_fixture_origin,
    recipe_id_by_name, technology_id_by_name,
};
use factory_app::ui::formatting::available_crafting_recipe_choices;
use factory_sim::{Direction, ItemStack, Simulation};

#[test]
fn completed_research_unlocks_recipe() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let automation = technology_id_by_name(sim.catalog(), "automation");
    let science_pack = item_id_by_name(sim.catalog(), "automation_science_pack");
    let assembling_machine = recipe_id_by_name(sim.catalog(), "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let lab_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");
    complete_research_by_name(&mut sim, "logistics");
    sim.select_research(automation)
        .expect("automation should be selectable");
    sim.entity_inventory_mut(lab_id)
        .expect("lab should expose inventory")
        .slots[0] = Some(ItemStack {
        item_id: science_pack,
        count: 20,
    });

    assert!(
        !available_crafting_recipe_choices(&sim)
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );

    for _ in 0..12_000 {
        sim.tick();
    }

    assert!(
        available_crafting_recipe_choices(&sim)
            .iter()
            .any(|recipe| recipe.id == assembling_machine)
    );
}
