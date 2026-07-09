use super::common::{
    entity_id_by_name, first_available_hotbar_slot, first_buildable_rect, hotbar_key_for_slot,
    item_id_by_name, test_app,
};
use bevy::prelude::*;
use factory_app::build::resources::{
    BlueprintLibraryWindowState, BuildPlacementState, BuildPlacementStatus, BuildSelection,
    HotbarState, PlannerState, PlannerTool,
};
use factory_app::resources::SimResource;
use factory_app::simulation::SimCommandRequest;
use factory_sim::SimCommand;
use std::time::Duration;

fn press_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

fn release_key(app: &mut App, key: KeyCode) {
    let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keyboard.clear_just_pressed(key);
    keyboard.release(key);
}

#[test]
fn x_key_toggles_deconstruction_planner_and_escape_clears_it() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    press_key(&mut app, KeyCode::KeyX);
    app.update();
    assert_eq!(
        app.world().resource::<PlannerState>().tool,
        PlannerTool::Deconstruct
    );

    release_key(&mut app, KeyCode::KeyX);
    press_key(&mut app, KeyCode::Escape);
    app.update();
    assert_eq!(
        app.world().resource::<PlannerState>().tool,
        PlannerTool::None
    );
}

#[test]
fn ctrl_b_toggles_blueprint_library_without_build_menu() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    press_key(&mut app, KeyCode::ControlLeft);
    press_key(&mut app, KeyCode::KeyB);
    app.update();

    assert!(app.world().resource::<BlueprintLibraryWindowState>().open);
    assert!(
        !app.world()
            .resource::<factory_app::build::resources::BuildMenuState>()
            .open
    );

    release_key(&mut app, KeyCode::ControlLeft);
    release_key(&mut app, KeyCode::KeyB);
    press_key(&mut app, KeyCode::Escape);
    app.update();
    assert!(!app.world().resource::<BlueprintLibraryWindowState>().open);
}

#[test]
fn selecting_a_build_item_deactivates_the_planner_tool() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let (slot_index, selection) = first_available_hotbar_slot(&app);

    press_key(&mut app, KeyCode::KeyX);
    app.update();
    assert_eq!(
        app.world().resource::<PlannerState>().tool,
        PlannerTool::Deconstruct
    );

    release_key(&mut app, KeyCode::KeyX);
    press_key(&mut app, hotbar_key_for_slot(slot_index));
    app.update();

    assert_eq!(
        app.world().resource::<PlannerState>().tool,
        PlannerTool::None
    );
    assert_eq!(
        app.world().resource::<BuildPlacementState>().selected,
        Some(selection)
    );
}

#[test]
fn zero_inventory_selection_stays_armed_for_ghost_planning() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    let selection = {
        let sim = &app.world().resource::<SimResource>().sim;
        let selection = BuildSelection {
            prototype_id: entity_id_by_name(sim.catalog(), "transport_belt"),
            item_id: item_id_by_name(sim.catalog(), "transport_belt"),
        };
        assert!(sim.is_entity_unlocked(selection.prototype_id));
        assert_eq!(sim.player_inventory().count(selection.item_id), 0);
        selection
    };
    let slot_index = 0;
    app.world_mut().resource_mut::<HotbarState>().slots[slot_index] = Some(selection);

    press_key(&mut app, hotbar_key_for_slot(slot_index));
    app.update();

    let build_state = app.world().resource::<BuildPlacementState>();
    assert_eq!(build_state.selected, Some(selection));
    assert!(matches!(
        build_state.last_status,
        BuildPlacementStatus::MissingInventory(_)
    ));
}

#[test]
fn ghost_commands_flow_through_the_fixed_tick() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    let (furnace, x, y) = {
        let sim = &app.world().resource::<SimResource>().sim;
        let furnace = entity_id_by_name(sim.catalog(), "stone_furnace");
        let (x, y) = first_buildable_rect(sim, furnace);
        (furnace, x, y)
    };

    app.world_mut()
        .write_message(SimCommandRequest(SimCommand::PlaceGhost {
            prototype_id: furnace,
            x,
            y,
            direction: factory_sim::Direction::North,
        }));
    app.update();

    let ghost_id = {
        let sim = &app.world().resource::<SimResource>().sim;
        assert_eq!(sim.construction().ghost_count(), 1);
        sim.construction()
            .ghost_at(x, y)
            .expect("ghost should occupy its tile")
            .id
    };

    app.world_mut()
        .write_message(SimCommandRequest(SimCommand::BuildGhost { ghost_id }));
    app.update();

    let sim = &app.world().resource::<SimResource>().sim;
    assert_eq!(sim.construction().ghost_count(), 0);
    assert!(
        sim.entities().occupancy().entity_at(x, y).is_some(),
        "built ghost should place a real entity"
    );
}
