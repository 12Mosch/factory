use super::common::{
    entity_id_by_name, first_buildable_rect, first_placeable_resource_rect, item_id_by_name,
    place_powered_fixture_origin,
};
use factory_app::build::resources::BuildPlacementState;
use factory_app::interaction::container_open::{
    container_open_input_allowed, opened_container_after_world_click,
};
use factory_app::placement::build::buildable_prototypes;
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

/// Clicking a placed container-like entity opens that entity. Every case places
/// a different prototype and expects the click to select exactly it; unusual
/// interactions (like the wagon below) stay in their own tests.
#[test]
fn opening_clicked_entity_selects_correct_entity() {
    enum Placement {
        BuildableRect,
        ResourceDrill,
        PoweredFixture,
    }

    for (prototype_name, placement) in [
        ("chest", Placement::BuildableRect),
        ("burner_mining_drill", Placement::ResourceDrill),
        ("stone_furnace", Placement::BuildableRect),
        ("assembling_machine", Placement::PoweredFixture),
        ("lab", Placement::PoweredFixture),
    ] {
        let mut sim = Simulation::new_test_world(123);
        let prototype = entity_id_by_name(sim.catalog(), prototype_name);
        let (x, y) = match placement {
            Placement::BuildableRect => first_buildable_rect(&sim, prototype),
            Placement::ResourceDrill => {
                let coal = item_id_by_name(sim.catalog(), "coal");
                first_placeable_resource_rect(&sim, prototype, coal)
            }
            Placement::PoweredFixture => place_powered_fixture_origin(&mut sim, 3, 3, (3, 1)),
        };
        let entity_id = factory_sim::placement::place(
            &mut sim,
            factory_sim::placement::EntityPlacementRequest {
                prototype_id: prototype,
                x,
                y,
                direction: Direction::North,
            },
        )
        .unwrap_or_else(|error| panic!("{prototype_name} should be placeable: {error:?}"));

        assert_eq!(
            opened_container_after_world_click(&sim, Some((x, y))),
            (Some(entity_id), None),
            "clicking {prototype_name} should select that entity"
        );
    }
}

/// A wagon stands on a rail, and a rail is an ordinary placed entity — so the
/// occupancy lookup would answer with the track and open nothing at all. The
/// click has to find the stock first.
#[test]
fn clicking_a_wagon_opens_the_rolling_stock_window_rather_than_the_rail() {
    let mut sim = Simulation::new_test_world(123);
    let straight = entity_id_by_name(sim.catalog(), "rail_straight");
    let (x, y) = first_buildable_rect(&sim, straight);
    for index in 0..6 {
        factory_sim::placement::place(
            &mut sim,
            factory_sim::placement::EntityPlacementRequest {
                prototype_id: straight,
                x,
                y: y + index * 2,
                direction: Direction::North,
            },
        )
        .expect("a straight run should be placeable at a buildable origin");
    }
    sim.tick();

    let wagon = entity_id_by_name(sim.catalog(), "cargo_wagon");
    let stock_id = sim
        .place_rolling_stock(wagon, x, y + 4)
        .expect("a wagon should fit on a six-piece run");
    sim.tick();
    let (wagon_x, wagon_y) = sim
        .rolling_stock_tile(stock_id)
        .expect("a placed wagon stands somewhere");

    assert_eq!(
        opened_container_after_world_click(&sim, Some((wagon_x, wagon_y))),
        (None, Some(stock_id))
    );
}
