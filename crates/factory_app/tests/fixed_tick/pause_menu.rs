use super::common::{sim_tick_and_hash, test_app};
use bevy::prelude::*;
use factory_app::save_load::SaveLoadWindowState;
use factory_app::simulation::AppPauseState;
use factory_app::ui::pause_menu::{
    PauseHudButton, PauseMenuAction, PauseMenuActionButton, PauseMenuState,
};
use factory_app::ui::save_load::{SaveLoadBackButton, SaveLoadModal, SaveLoadSlotList};
use std::time::Duration;

#[test]
fn save_load_manager_is_a_separate_page_reached_from_pause() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    press_key(&mut app, KeyCode::Escape);
    app.update();
    release_key(&mut app, KeyCode::Escape);
    app.update();

    assert!(app.world().resource::<PauseMenuState>().open);
    assert!(!app.world().resource::<SaveLoadWindowState>().open);
    assert_eq!(component_count::<SaveLoadModal>(&mut app), 0);

    let save_load = action_button(&mut app, PauseMenuAction::SaveLoad);
    press_button(&mut app, save_load);

    assert!(!app.world().resource::<PauseMenuState>().open);
    assert!(app.world().resource::<SaveLoadWindowState>().open);
    assert_eq!(component_count::<SaveLoadModal>(&mut app), 1);
    assert_eq!(component_count::<SaveLoadSlotList>(&mut app), 1);

    let back = single_button::<SaveLoadBackButton>(&mut app);
    press_button(&mut app, back);

    assert!(app.world().resource::<PauseMenuState>().open);
    assert!(!app.world().resource::<SaveLoadWindowState>().open);
    assert_eq!(component_count::<SaveLoadModal>(&mut app), 0);
}

#[test]
fn escape_from_save_load_returns_to_pause() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();
    app.world_mut().resource_mut::<SaveLoadWindowState>().open = true;
    app.update();

    press_key(&mut app, KeyCode::Escape);
    app.update();

    assert!(app.world().resource::<PauseMenuState>().open);
    assert!(!app.world().resource::<SaveLoadWindowState>().open);
}

#[test]
fn escape_pause_freezes_render_frames_and_resumes_fixed_ticks() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    press_key(&mut app, KeyCode::Escape);
    app.update();
    assert!(app.world().resource::<AppPauseState>().is_paused());
    assert!(app.world().resource::<PauseMenuState>().open);
    let frozen = sim_tick_and_hash(&app);

    release_key(&mut app, KeyCode::Escape);
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(sim_tick_and_hash(&app), frozen);

    press_key(&mut app, KeyCode::Escape);
    app.update();
    assert!(!app.world().resource::<AppPauseState>().is_paused());
    assert!(!app.world().resource::<PauseMenuState>().open);
    assert!(sim_tick_and_hash(&app).0 > frozen.0);
}

#[test]
fn hud_pause_and_menu_resume_control_the_same_state() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    app.update();

    let pause = single_button::<PauseHudButton>(&mut app);
    press_button(&mut app, pause);
    assert!(app.world().resource::<AppPauseState>().is_paused());
    assert!(app.world().resource::<PauseMenuState>().open);

    let resume = action_button(&mut app, PauseMenuAction::Resume);
    press_button(&mut app, resume);
    assert!(!app.world().resource::<AppPauseState>().is_paused());
    assert!(!app.world().resource::<PauseMenuState>().open);
}

fn action_button(app: &mut App, action: PauseMenuAction) -> Entity {
    let world = app.world_mut();
    let mut buttons = world.query::<(Entity, &PauseMenuActionButton)>();
    buttons
        .iter(world)
        .find_map(|(entity, button)| (button.action == action).then_some(entity))
        .expect("pause menu action should exist")
}

fn single_button<T: Component>(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut buttons = world.query_filtered::<Entity, (With<Button>, With<T>)>();
    buttons
        .single(world)
        .expect("expected exactly one matching button")
}

fn component_count<T: Component>(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut components = world.query_filtered::<Entity, With<T>>();
    components.iter(world).count()
}

fn press_button(app: &mut App, entity: Entity) {
    *app.world_mut()
        .entity_mut(entity)
        .get_mut::<Interaction>()
        .expect("button should have interaction") = Interaction::Pressed;
    app.update();
}

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
