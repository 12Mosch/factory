use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::FactoryAppPlugin;
use factory_app::rendering::resources::ResourceRenderCache;
use factory_app::resources::{
    BuildPlacementState, BuildSelection, MapTextureCache, OpenContainer, SimResource,
};
use factory_app::save_load::{
    LOAD_SAVE_SLOTS, MANUAL_SAVE_SLOTS, PendingSaveJobs, PresentationReloadToken, SaveLoadConfig,
    SaveLoadStatus, SaveLoadStatusKind, SaveLoadTab, SaveLoadWindowState, SaveSlotKind, slot_path,
};
use factory_app::ui::save_load::{SaveSlotAction, SaveSlotButton};
use factory_data::{EntityPrototypeId, ItemId};
use factory_sim::{EntityId, SAVE_VERSION, load_from_bytes, save_to_bytes};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn save_load_config_has_three_manual_slots() {
    assert_eq!(
        MANUAL_SAVE_SLOTS,
        [
            SaveSlotKind::Manual(1),
            SaveSlotKind::Manual(2),
            SaveSlotKind::Manual(3)
        ]
    );
    assert!(LOAD_SAVE_SLOTS.contains(&SaveSlotKind::Quick));
    assert!(LOAD_SAVE_SLOTS.contains(&SaveSlotKind::Auto));
}

#[test]
fn f5_quick_save_writes_loadable_exact_state() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "f5_quick_save");
    run_until_tick(&mut app, 3);
    freeze_time(&mut app);
    let captured = sim_tick_and_hash(&app);

    press_key(&mut app, KeyCode::F5);
    app.update();
    drain_save_jobs(&mut app);

    let config = app.world().resource::<SaveLoadConfig>();
    let bytes = fs::read(slot_path(config, SaveSlotKind::Quick)).expect("quicksave should exist");
    let loaded = load_from_bytes(&bytes).expect("quicksave should be loadable");

    assert_eq!((loaded.tick_count(), loaded.state_hash()), captured);
}

#[test]
fn f9_quick_load_restores_exact_state() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "f9_quick_load");
    run_until_tick(&mut app, 4);
    let saved = sim_tick_and_hash(&app);
    write_slot_save(&app, SaveSlotKind::Quick);

    run_until_tick(&mut app, saved.0 + 5);
    assert_ne!(sim_tick_and_hash(&app), saved);
    freeze_time(&mut app);

    press_key(&mut app, KeyCode::F9);
    app.update();

    assert_eq!(sim_tick_and_hash(&app), saved);
}

#[test]
fn manual_save_button_writes_slot_file() {
    let mut app = test_app(Duration::ZERO, "manual_save_button");
    open_save_load_menu(&mut app, SaveLoadTab::Save);
    press_slot_button(&mut app, SaveSlotKind::Manual(1), SaveSlotAction::Save);
    app.update();
    drain_save_jobs(&mut app);

    let config = app.world().resource::<SaveLoadConfig>();
    let path = slot_path(config, SaveSlotKind::Manual(1));
    let bytes = fs::read(path).expect("manual slot should exist");
    load_from_bytes(&bytes).expect("manual slot should load");
}

#[test]
fn manual_load_button_restores_slot_file() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "manual_load_button");
    run_until_tick(&mut app, 4);
    let saved = sim_tick_and_hash(&app);
    write_slot_save(&app, SaveSlotKind::Manual(1));

    run_until_tick(&mut app, saved.0 + 5);
    freeze_time(&mut app);
    open_save_load_menu(&mut app, SaveLoadTab::Load);
    press_slot_button(&mut app, SaveSlotKind::Manual(1), SaveSlotAction::Load);
    app.update();

    assert_eq!(sim_tick_and_hash(&app), saved);
}

#[test]
fn version_mismatch_reports_clear_error_and_keeps_current_sim() {
    let mut app = test_app(Duration::ZERO, "version_mismatch");
    let before = sim_tick_and_hash(&app);
    let mut bytes =
        save_to_bytes(&app.world().resource::<SimResource>().sim).expect("current sim should save");
    let found_version = SAVE_VERSION + 1;
    bytes[8..12].copy_from_slice(&found_version.to_le_bytes());
    write_slot_bytes(&app, SaveSlotKind::Manual(1), &bytes);

    open_save_load_menu(&mut app, SaveLoadTab::Load);
    press_slot_button(&mut app, SaveSlotKind::Manual(1), SaveSlotAction::Load);
    app.update();

    assert_eq!(sim_tick_and_hash(&app), before);
    let status = app.world().resource::<SaveLoadStatus>();
    assert_eq!(status.kind, SaveLoadStatusKind::Error);
    let message = status.message.as_deref().expect("error should be visible");
    assert!(message.contains("save version"));
    assert!(message.contains(&found_version.to_string()));
    assert!(message.contains(&SAVE_VERSION.to_string()));
}

#[test]
fn autosave_uses_background_job_and_writes_file() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "autosave");
    app.world_mut()
        .resource_mut::<SaveLoadConfig>()
        .autosave_interval_ticks = 1;

    for _ in 0..10 {
        app.update();
        if !app.world().resource::<PendingSaveJobs>().is_empty() {
            break;
        }
    }
    assert!(!app.world().resource::<PendingSaveJobs>().is_empty());
    drain_save_jobs(&mut app);

    let config = app.world().resource::<SaveLoadConfig>();
    let bytes = fs::read(slot_path(config, SaveSlotKind::Auto)).expect("autosave should exist");
    load_from_bytes(&bytes).expect("autosave should load");
}

#[test]
fn load_resets_transient_app_state() {
    let mut app = test_app(Duration::ZERO, "load_resets_transient");
    write_slot_save(&app, SaveSlotKind::Quick);
    {
        let mut build_state = app.world_mut().resource_mut::<BuildPlacementState>();
        build_state.selected = Some(BuildSelection {
            prototype_id: EntityPrototypeId::new(0),
            item_id: ItemId::new(0),
        });
    }
    app.world_mut().resource_mut::<OpenContainer>().entity_id = Some(EntityId::new(999));
    app.world_mut().resource_mut::<SaveLoadWindowState>().open = true;

    press_key(&mut app, KeyCode::F9);
    app.update();

    assert_eq!(app.world().resource::<BuildPlacementState>().selected, None);
    assert_eq!(app.world().resource::<OpenContainer>().entity_id, None);
    assert!(!app.world().resource::<SaveLoadWindowState>().open);
}

#[test]
fn load_invalidates_render_caches() {
    let mut app = test_app(Duration::ZERO, "load_invalidates_caches");
    write_slot_save(&app, SaveSlotKind::Quick);
    app.world_mut().insert_resource(MapTextureCache {
        handle: None,
        bounds: Some(Default::default()),
        last_player_tile: Some((12, 34)),
        last_chunk_revision: 66,
        last_resource_revision: 99,
        last_entity_signature: 88,
        last_revealed_signature: 77,
        last_debug_flags: (true, true),
    });
    app.world_mut().insert_resource(ResourceRenderCache {
        last_resource_revision: Some(42),
        last_visible_revision: 13,
        sprite_entities: Default::default(),
        label_entities: Default::default(),
        show_amount_labels: true,
    });
    let before_token = app.world().resource::<PresentationReloadToken>().value;

    press_key(&mut app, KeyCode::F9);
    app.update();

    let map_cache = app.world().resource::<MapTextureCache>();
    assert_eq!(map_cache.bounds, None);
    assert_eq!(map_cache.last_player_tile, None);
    assert_eq!(map_cache.last_resource_revision, 0);
    assert_ne!(
        app.world()
            .resource::<ResourceRenderCache>()
            .last_resource_revision,
        Some(42)
    );
    assert_eq!(
        app.world().resource::<PresentationReloadToken>().value,
        before_token + 1
    );
}

fn test_app(frame_duration: Duration, test_name: &str) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration));
    app.world_mut().insert_resource(SaveLoadConfig {
        root_dir: unique_temp_dir(test_name),
        autosave_interval_ticks: 5 * 60 * 60,
    });
    app
}

fn unique_temp_dir(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "factory_save_load_{test_name}_{}_{}",
        std::process::id(),
        nanos
    ))
}

fn freeze_time(app: &mut App) {
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
}

fn run_until_tick(app: &mut App, target_tick: u64) {
    while app.world().resource::<SimResource>().sim.tick_count() < target_tick {
        app.update();
    }
}

fn sim_tick_and_hash(app: &App) -> (u64, u64) {
    let sim = &app.world().resource::<SimResource>().sim;
    (sim.tick_count(), sim.state_hash())
}

fn press_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

fn open_save_load_menu(app: &mut App, tab: SaveLoadTab) {
    {
        let mut window = app.world_mut().resource_mut::<SaveLoadWindowState>();
        window.open = true;
        window.tab = tab;
    }
    app.update();
}

fn press_slot_button(app: &mut App, slot: SaveSlotKind, action: SaveSlotAction) {
    let mut query = app
        .world_mut()
        .query::<(&SaveSlotButton, &mut Interaction)>();
    let mut pressed = false;
    for (button, mut interaction) in query.iter_mut(app.world_mut()) {
        if button.slot == slot && button.action == action {
            *interaction = Interaction::Pressed;
            pressed = true;
        }
    }
    assert!(pressed, "expected {action:?} button for {slot:?}");
}

fn drain_save_jobs(app: &mut App) {
    for _ in 0..200 {
        if app.world().resource::<PendingSaveJobs>().is_empty() {
            return;
        }
        app.update();
        std::thread::yield_now();
    }
    panic!("save jobs did not drain");
}

fn write_slot_save(app: &App, slot: SaveSlotKind) {
    let bytes =
        save_to_bytes(&app.world().resource::<SimResource>().sim).expect("current sim should save");
    write_slot_bytes(app, slot, &bytes);
}

fn write_slot_bytes(app: &App, slot: SaveSlotKind, bytes: &[u8]) {
    let config = app.world().resource::<SaveLoadConfig>();
    let path = slot_path(config, slot);
    fs::create_dir_all(path.parent().expect("slot path should have a parent"))
        .expect("save dir should be created");
    fs::write(path, bytes).expect("slot bytes should be written");
}
