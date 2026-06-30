use super::common::{
    complete_research_by_name, entity_id_by_name, first_available_build_selection,
    first_buildable_rect, hotbar_key_for_slot, item_id_by_name, technology_id_by_name, test_app,
};
use bevy::prelude::*;
use factory_app::placement::build::{buildable_prototypes, place_selected_building_at_tile};
use factory_app::resources::{
    BuildPlacementState, BuildPlacementStatus, BuildSelection, SimResource,
};
use factory_data::{EntityKind, PrototypeCatalog};
use factory_sim::{Direction, Inventory, Simulation};
use std::time::Duration;

#[test]
fn buildable_prototypes_include_placeable_item_backed_entities() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let buildables = buildable_prototypes(&catalog);
    let buildable_names = buildables
        .iter()
        .map(|buildable| {
            catalog.entities[buildable.prototype_id.index()]
                .name
                .as_str()
        })
        .collect::<Vec<_>>();

    for expected in [
        "chest",
        "transport_belt",
        "fast_transport_belt",
        "express_transport_belt",
        "splitter",
        "fast_splitter",
        "express_splitter",
        "inserter",
        "stone_furnace",
        "burner_mining_drill",
        "assembling_machine",
        "lab",
        "underground_belt_entrance",
        "underground_belt_exit",
        "fast_underground_belt_entrance",
        "fast_underground_belt_exit",
        "express_underground_belt_entrance",
        "express_underground_belt_exit",
        "pipe",
        "storage_tank",
    ] {
        assert!(
            buildable_names.contains(&expected),
            "missing buildable prototype {expected}"
        );
    }
    assert!(buildables.iter().all(|buildable| {
        let entity = &catalog.entities[buildable.prototype_id.index()];
        entity.entity_kind != EntityKind::ResourcePatch
            && entity.build_item == Some(buildable.item_id)
    }));
}

#[test]
fn number_key_selects_hotbar_slot_without_placing() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let before_entities = app
        .world()
        .resource::<SimResource>()
        .sim
        .entities()
        .placed_len();
    let slot = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| sim.player_inventory().count(buildable.item_id) > 0)
            .expect("starting inventory should include at least one buildable item")
    };

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot.slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(
        build_state.selected,
        Some(BuildSelection {
            prototype_id: slot.prototype_id,
            item_id: slot.item_id,
        })
    );
    assert_eq!(
        app.world()
            .resource::<SimResource>()
            .sim
            .entities()
            .placed_len(),
        before_entities
    );
}

#[test]
fn rotate_key_updates_build_direction() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let selection = first_available_build_selection(&app);
    app.world_mut()
        .resource_mut::<BuildPlacementState>()
        .selected = Some(selection);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyR);
    app.update();

    assert_eq!(
        app.world().resource::<BuildPlacementState>().direction,
        Direction::East
    );
}

#[test]
fn escape_clears_build_selection() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let selection = first_available_build_selection(&app);
    app.world_mut()
        .resource_mut::<BuildPlacementState>()
        .selected = Some(selection);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
}

#[test]
fn build_bar_rejects_locked_buildable_and_allows_after_research() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (assembler_entity, assembler_item, automation, slot_index) = {
        let sim = &app.world().resource::<SimResource>().sim;
        let assembler_entity = entity_id_by_name(sim.catalog(), "assembling_machine");
        let assembler_item = item_id_by_name(sim.catalog(), "assembling_machine");
        let automation = technology_id_by_name(sim.catalog(), "automation");
        let slot_index = buildable_prototypes(sim.catalog())
            .into_iter()
            .find(|buildable| buildable.prototype_id == assembler_entity)
            .expect("assembling machine should be buildable")
            .slot_index;
        (assembler_entity, assembler_item, automation, slot_index)
    };
    {
        let catalog = app.world().resource::<SimResource>().sim.catalog().clone();
        app.world_mut()
            .resource_mut::<SimResource>()
            .sim
            .player_inventory_mut()
            .insert(&catalog, assembler_item, 1)
            .expect("test inventory should accept assembler");
    }

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(build_state.selected, None);
    assert!(matches!(
        build_state.last_status,
        BuildPlacementStatus::Locked(_)
    ));

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(hotbar_key_for_slot(slot_index));
    app.update();
    {
        let mut sim = app.world_mut().resource_mut::<SimResource>();
        complete_research_by_name(&mut sim.sim, "logistics");
        sim.sim
            .select_research(automation)
            .expect("automation should be selectable");
        sim.sim
            .add_research_units(20)
            .expect("automation should complete");
    }
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(
        app.world().resource::<BuildPlacementState>().selected,
        Some(BuildSelection {
            prototype_id: assembler_entity,
            item_id: assembler_item,
        })
    );
}

#[test]
fn place_selected_building_consumes_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(sim.catalog(), "transport_belt");
    let belt_item = item_id_by_name(sim.catalog(), "transport_belt");
    let (x, y) = first_buildable_rect(&sim, belt);
    *sim.player_inventory_mut() = Inventory::player();
    let catalog = sim.catalog().clone();
    sim.player_inventory_mut()
        .insert(&catalog, belt_item, 1)
        .expect("test inventory should accept belt");
    let before_entities = sim.entities().placed_len();

    let status = place_selected_building_at_tile(
        &mut sim,
        BuildSelection {
            prototype_id: belt,
            item_id: belt_item,
        },
        Direction::North,
        x,
        y,
    );

    assert!(matches!(status, BuildPlacementStatus::Placed(_)));
    assert_eq!(sim.player_inventory().count(belt_item), 0);
    assert_eq!(sim.entities().placed_len(), before_entities + 1);
}

#[test]
fn failed_selected_building_placement_keeps_inventory() {
    let mut sim = Simulation::new_test_world(123);
    let belt = entity_id_by_name(sim.catalog(), "transport_belt");
    let belt_item = item_id_by_name(sim.catalog(), "transport_belt");
    let (x, y) = first_buildable_rect(&sim, belt);
    *sim.player_inventory_mut() = Inventory::player();
    let catalog = sim.catalog().clone();
    sim.player_inventory_mut()
        .insert(&catalog, belt_item, 1)
        .expect("test inventory should accept belt");
    sim.place_entity(belt, x, y, Direction::North)
        .expect("blocking belt should be placeable");

    let status = place_selected_building_at_tile(
        &mut sim,
        BuildSelection {
            prototype_id: belt,
            item_id: belt_item,
        },
        Direction::North,
        x,
        y,
    );

    assert!(matches!(status, BuildPlacementStatus::CannotPlace(_)));
    assert_eq!(sim.player_inventory().count(belt_item), 1);
}
