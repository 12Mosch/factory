use super::common::{
    complete_research_by_name, entity_id_by_name, first_available_build_selection,
    first_available_hotbar_slot, first_buildable_rect, hotbar_key_for_slot, item_id_by_name,
    technology_id_by_name, test_app,
};
use bevy::prelude::*;
use factory_app::placement::build::{
    buildable_prototypes, default_hotbar_slots, place_selected_building_at_tile,
};
use factory_app::resources::{
    BuildPlacementState, BuildPlacementStatus, BuildSelection, HOTBAR_SLOT_COUNT, HotbarState,
    SimResource,
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
fn default_hotbar_holds_first_ten_buildables() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    let expected = {
        let sim = &app.world().resource::<SimResource>().sim;
        buildable_prototypes(sim.catalog())
            .into_iter()
            .take(HOTBAR_SLOT_COUNT)
            .map(|buildable| Some(buildable.selection()))
            .collect::<Vec<_>>()
    };
    let hotbar = app.world().resource::<HotbarState>();

    assert_eq!(hotbar.slots.len(), HOTBAR_SLOT_COUNT);
    assert_eq!(hotbar.slots.to_vec(), expected);
}

#[test]
fn hotbar_add_and_remove_assigns_first_empty_slot() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let mut hotbar = HotbarState {
        slots: default_hotbar_slots(&catalog),
    };
    let buildables = buildable_prototypes(&catalog);
    assert!(
        buildables.len() > HOTBAR_SLOT_COUNT,
        "catalog should have more buildables than hotbar slots"
    );
    let extra = buildables[HOTBAR_SLOT_COUNT].selection();
    let second = buildables[1].selection();

    assert_eq!(hotbar.assign_to_first_empty(extra), None);
    assert!(hotbar.remove(second));
    assert_eq!(hotbar.slot(1), None);
    assert_eq!(hotbar.assign_to_first_empty(extra), Some(1));
    assert_eq!(hotbar.slot(1), Some(extra));
    assert_eq!(hotbar.assign_to_first_empty(extra), Some(1));
    assert!(!hotbar.remove(second));
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
    let (slot_index, selection) = first_available_hotbar_slot(&app);

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(build_state.selected, Some(selection));
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
fn empty_hotbar_slot_selection_clears_stale_status() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let slot_index = 0;
    app.world_mut().resource_mut::<HotbarState>().slots[slot_index] = None;
    app.world_mut()
        .resource_mut::<BuildPlacementState>()
        .last_status = BuildPlacementStatus::Locked("Assembler locked".to_string());

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(hotbar_key_for_slot(slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(build_state.selected, None);
    assert_eq!(build_state.last_status, BuildPlacementStatus::Ready);
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
    let (assembler_entity, assembler_item, automation) = {
        let sim = &app.world().resource::<SimResource>().sim;
        let assembler_entity = entity_id_by_name(sim.catalog(), "assembling_machine");
        let assembler_item = item_id_by_name(sim.catalog(), "assembling_machine");
        let automation = technology_id_by_name(sim.catalog(), "automation");
        (assembler_entity, assembler_item, automation)
    };
    let slot_index = 0;
    app.world_mut().resource_mut::<HotbarState>().slots[slot_index] = Some(BuildSelection {
        prototype_id: assembler_entity,
        item_id: assembler_item,
    });
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
    factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: belt,
            x,
            y,
            direction: Direction::North,
        },
    )
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

    assert_eq!(
        status,
        BuildPlacementStatus::CannotPlace("Entity already there".to_string())
    );
    assert_eq!(sim.player_inventory().count(belt_item), 1);
}
