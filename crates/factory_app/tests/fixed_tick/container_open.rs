use super::common::{
    entity_id_by_name, first_buildable_rect, first_placeable_resource_rect, item_id_by_name,
    place_powered_fixture_origin,
};
use factory_app::interaction::container_open::{
    container_open_input_allowed, opened_container_after_world_click,
};
use factory_app::placement::build::buildable_prototypes;
use factory_app::build::resources::BuildPlacementState;
use factory_data::PrototypeCatalog;
use factory_sim::{Direction, Simulation};

#[test]
fn container_open_ignores_click_when_building_selected() {
    let mut build_state = BuildPlacementState::default();
    assert!(container_open_input_allowed(&build_state));

    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let buildable = buildable_prototypes(&catalog)
        .into_iter()
        .next()
        .expect("catalog should include at least one buildable");
    build_state.selected = Some(buildable.selection());

    assert!(!container_open_input_allowed(&build_state));
}

#[test]
fn opening_clicked_chest_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(sim.catalog(), "chest");
    let (x, y) = first_buildable_rect(&sim, chest);
    let entity_id = sim
        .place_entity(chest, x, y, Direction::North)
        .expect("chest should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_burner_drill_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let drill = entity_id_by_name(sim.catalog(), "burner_mining_drill");
    let coal = item_id_by_name(sim.catalog(), "coal");
    let (x, y) = first_placeable_resource_rect(&sim, drill, coal);
    let entity_id = sim
        .place_entity(drill, x, y, Direction::North)
        .expect("burner drill should be placeable over resources");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_furnace_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
    let (x, y) = first_buildable_rect(&sim, furnace);
    let entity_id = sim
        .place_entity(furnace, x, y, Direction::North)
        .expect("furnace should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_assembler_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let assembler = entity_id_by_name(sim.catalog(), "assembling_machine");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(assembler, x, y, Direction::North)
        .expect("assembler should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}

#[test]
fn opening_clicked_lab_selects_correct_entity() {
    let mut sim = Simulation::new_test_world(123);
    let lab = entity_id_by_name(sim.catalog(), "lab");
    let (x, y) = place_powered_fixture_origin(&mut sim, 3, 3, (3, 1));
    let entity_id = sim
        .place_entity(lab, x, y, Direction::North)
        .expect("lab should be placeable");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((x, y))),
        Some(entity_id)
    );
}
