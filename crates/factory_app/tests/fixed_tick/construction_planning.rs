use super::common::{entity_id_by_name, first_buildable_rect, test_app};
use bevy::prelude::*;
use factory_app::build::resources::{BlueprintLibraryWindowState, PlannerState, PlannerTool};
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
