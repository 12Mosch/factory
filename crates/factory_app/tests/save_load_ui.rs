use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use factory_app::FactoryAppPlugin;
use factory_app::build::resources::{BuildPlacementState, BuildSelection};
use factory_app::resources::SimResource;
use factory_app::save_load::{
    PendingSaveConfirmation, PendingSaveJobs, SaveCatalog, SaveCompatibility, SaveKind,
    SaveLoadConfig, SaveLoadMetrics, SaveLoadTab, SaveLoadWindowState, decode_container,
    encode_container,
};
use factory_app::ui::resources::OpenContainer;
use factory_app::ui::save_load::{
    SaveConfirmationButton, SaveCreateButton, SaveEntryAction, SaveEntryButton,
};
use factory_data::{EntityPrototypeId, ItemId};
use factory_sim::{EntityId, SAVE_VERSION, load_from_bytes, save_to_bytes};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn defaults_use_five_five_minute_autosaves() {
    let config = SaveLoadConfig::default();
    assert_eq!(config.autosave_slot_count, 5);
    assert_eq!(config.autosave_interval_ticks, 5 * 60 * 60);
}

#[test]
fn f5_writes_container_with_exact_simulation_payload() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "f5");
    run_until_tick(&mut app, 3);
    freeze_time(&mut app);
    let captured = sim_tick_and_hash(&app);

    press_key(&mut app, KeyCode::F5);
    app.update();
    drain_save_jobs(&mut app);

    let path = app
        .world()
        .resource::<SaveLoadConfig>()
        .root_dir
        .join("quicksave.factsim");
    let bytes = fs::read(path).unwrap();
    let (metadata, payload) = decode_container(&bytes).unwrap();
    assert_eq!(metadata.kind, SaveKind::Quicksave);
    let loaded = load_from_bytes(payload).unwrap();
    assert_eq!((loaded.tick_count(), loaded.state_hash()), captured);
}

#[test]
fn f9_reads_existing_raw_quicksave_and_resets_transient_state() {
    let mut app = test_app(Duration::ZERO, "raw_quickload");
    let saved = sim_tick_and_hash(&app);
    write_raw_quicksave(&app);
    app.update();
    {
        let mut build = app.world_mut().resource_mut::<BuildPlacementState>();
        build.selected = Some(BuildSelection {
            prototype_id: EntityPrototypeId::new(0),
            item_id: ItemId::new(0),
        });
    }
    app.world_mut().resource_mut::<OpenContainer>().entity_id = Some(EntityId::new(999));
    press_key(&mut app, KeyCode::F9);
    app.update();
    assert_eq!(sim_tick_and_hash(&app), saved);
    assert!(
        app.world()
            .resource::<BuildPlacementState>()
            .selected
            .is_none()
    );
    assert!(app.world().resource::<OpenContainer>().entity_id.is_none());
}

#[test]
fn named_save_creation_and_duplicate_require_confirmation() {
    let mut app = test_app(Duration::ZERO, "named_duplicate");
    app.update();
    create_named_save(&mut app, "  Main Factory  ");
    drain_save_jobs(&mut app);
    let path = {
        let entry = app
            .world()
            .resource::<SaveCatalog>()
            .entries()
            .iter()
            .find(|entry| entry.metadata.kind == SaveKind::Named)
            .unwrap();
        assert_eq!(entry.metadata.display_name, "Main Factory");
        entry.path().to_path_buf()
    };
    let original = fs::read(&path).unwrap();

    create_named_save(&mut app, "main factory");
    assert!(matches!(
        app.world().resource::<PendingSaveConfirmation>(),
        PendingSaveConfirmation::Overwrite(_)
    ));
    assert!(app.world().resource::<PendingSaveJobs>().is_empty());
    assert_eq!(fs::read(&path).unwrap(), original);

    press_confirmation(&mut app, false);
    app.update();
    assert_eq!(
        *app.world().resource::<PendingSaveConfirmation>(),
        PendingSaveConfirmation::None
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn incompatible_named_save_stays_visible_and_deletable() {
    let mut app = test_app(Duration::ZERO, "incompatible_delete");
    app.update();
    create_named_save(&mut app, "Old World");
    drain_save_jobs(&mut app);
    let path = app.world().resource::<SaveCatalog>().entries()[0]
        .path()
        .to_path_buf();
    let bytes = fs::read(&path).unwrap();
    let (metadata, payload) = decode_container(&bytes).unwrap();
    let mut payload = payload.to_vec();
    payload[8..12].copy_from_slice(&(SAVE_VERSION - 1).to_le_bytes());
    fs::write(&path, encode_container(&metadata, &payload).unwrap()).unwrap();
    refresh_manager(&mut app);

    let entry = &app.world().resource::<SaveCatalog>().entries()[0];
    assert!(matches!(
        entry.compatibility,
        SaveCompatibility::SaveFormatOlder { .. }
    ));
    assert!(!entry.compatibility.can_load());
    let id = entry.id.clone();
    press_entry(&mut app, &id, SaveEntryAction::Delete);
    app.update();
    press_confirmation(&mut app, true);
    app.update();
    assert!(!path.exists());
}

#[test]
fn malformed_metadata_falls_back_without_blocking_load() {
    let mut app = test_app(Duration::ZERO, "metadata_fallback");
    app.update();
    let expected = sim_tick_and_hash(&app);
    create_named_save(&mut app, "Fallback World");
    drain_save_jobs(&mut app);
    let path = app.world().resource::<SaveCatalog>().entries()[0]
        .path()
        .to_path_buf();
    let mut bytes = fs::read(&path).unwrap();
    let metadata_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    bytes[16..16 + metadata_len].fill(b'!');
    fs::write(&path, bytes).unwrap();
    refresh_manager(&mut app);

    let entry = &app.world().resource::<SaveCatalog>().entries()[0];
    assert!(!entry.metadata_available);
    assert!(entry.compatibility.can_load());
    let id = entry.id.clone();
    app.world_mut().resource_mut::<SaveLoadWindowState>().tab = SaveLoadTab::Load;
    app.update();
    press_entry(&mut app, &id, SaveEntryAction::Load);
    app.update();
    assert_eq!(sim_tick_and_hash(&app), expected);
}

#[test]
fn display_name_never_becomes_a_filesystem_path() {
    let mut app = test_app(Duration::ZERO, "opaque_path");
    app.update();
    create_named_save(&mut app, "../escaped world");
    drain_save_jobs(&mut app);
    let root = app.world().resource::<SaveLoadConfig>().root_dir.clone();
    let entry = &app.world().resource::<SaveCatalog>().entries()[0];
    assert!(entry.path().starts_with(&root));
    assert!(
        entry
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("manual-")
    );
    assert!(!root.parent().unwrap().join("escaped world").exists());
}

#[test]
fn autosave_fills_generations_before_rotation() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "autosave_rotation");
    {
        let mut config = app.world_mut().resource_mut::<SaveLoadConfig>();
        config.autosave_interval_ticks = 1;
        config.autosave_slot_count = 5;
    }
    for generation in 1..=5 {
        run_until_jobs_start(&mut app);
        drain_save_jobs(&mut app);
        let path = app
            .world()
            .resource::<SaveLoadConfig>()
            .root_dir
            .join(format!("autosave-{generation}.factsim"));
        assert!(path.is_file());
    }
}

#[test]
fn catalog_ignores_old_slots_temps_backups_and_old_autosave() {
    let mut app = test_app(Duration::ZERO, "ignored_files");
    let root = app.world().resource::<SaveLoadConfig>().root_dir.clone();
    fs::create_dir_all(&root).unwrap();
    for name in [
        "slot_1.factsim",
        "slot_2.factsim",
        "slot_3.factsim",
        "autosave.factsim",
        "manual-x.factsim.tmp-1",
        "quicksave.factsim.bak-1",
        "notes.txt",
    ] {
        fs::write(root.join(name), b"ignored").unwrap();
    }
    app.update();
    assert!(app.world().resource::<SaveCatalog>().entries().is_empty());
}

#[test]
fn background_submission_remains_non_blocking_and_metrics_populate() {
    let mut app = test_app(Duration::ZERO, "metrics");
    press_key(&mut app, KeyCode::F5);
    app.update();
    assert!(
        app.world()
            .resource::<SaveLoadMetrics>()
            .last_request_submission_ms
            < 50.0
    );
    drain_save_jobs(&mut app);
    let metrics = app.world().resource::<SaveLoadMetrics>();
    assert!(metrics.last_bytes > 0);
    assert!(metrics.last_total_ms >= metrics.last_write_ms);
}

fn test_app(frame_duration: Duration, name: &str) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FactoryAppPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame_duration));
    app.world_mut().insert_resource(SaveLoadConfig {
        root_dir: unique_temp_dir(name),
        autosave_interval_ticks: 5 * 60 * 60,
        autosave_slot_count: 5,
    });
    app
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "factory_save_load_{name}_{}_{nanos}",
        std::process::id()
    ))
}

fn create_named_save(app: &mut App, name: &str) {
    {
        let mut window = app.world_mut().resource_mut::<SaveLoadWindowState>();
        window.open = true;
        window.tab = SaveLoadTab::Save;
        window.name_buffer = name.into();
        window.refresh_on_open = true;
    }
    app.update();
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Interaction, With<SaveCreateButton>>();
    *query.single_mut(app.world_mut()).unwrap() = Interaction::Pressed;
    app.update();
}

fn refresh_manager(app: &mut App) {
    app.world_mut()
        .resource_mut::<SaveLoadWindowState>()
        .refresh_on_open = true;
    app.update();
}

fn press_entry(app: &mut App, id: &factory_app::save_load::SaveId, action: SaveEntryAction) {
    let mut query = app
        .world_mut()
        .query::<(&SaveEntryButton, &mut Interaction)>();
    let mut found = false;
    for (button, mut interaction) in query.iter_mut(app.world_mut()) {
        if &button.id == id && button.action == action {
            *interaction = Interaction::Pressed;
            found = true;
        }
    }
    assert!(found);
}

fn press_confirmation(app: &mut App, confirm: bool) {
    app.update();
    let mut query = app
        .world_mut()
        .query::<(&SaveConfirmationButton, &mut Interaction)>();
    let mut found = false;
    for (button, mut interaction) in query.iter_mut(app.world_mut()) {
        if button.0 == confirm {
            *interaction = Interaction::Pressed;
            found = true;
        }
    }
    assert!(found);
}

fn write_raw_quicksave(app: &App) {
    let bytes = save_to_bytes(&app.world().resource::<SimResource>().read()).unwrap();
    let path = app
        .world()
        .resource::<SaveLoadConfig>()
        .root_dir
        .join("quicksave.factsim");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn run_until_jobs_start(app: &mut App) {
    for _ in 0..20 {
        app.update();
        if !app.world().resource::<PendingSaveJobs>().is_empty() {
            return;
        }
    }
    panic!("autosave did not start");
}

fn drain_save_jobs(app: &mut App) {
    for _ in 0..300 {
        if app.world().resource::<PendingSaveJobs>().is_empty() {
            return;
        }
        app.update();
        std::thread::yield_now();
    }
    panic!("save jobs did not drain");
}

fn run_until_tick(app: &mut App, tick: u64) {
    while app.world().resource::<SimResource>().read().tick_count() < tick {
        app.update();
    }
}
fn freeze_time(app: &mut App) {
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
}
fn sim_tick_and_hash(app: &App) -> (u64, u64) {
    let sim = app.world().resource::<SimResource>().read();
    (sim.tick_count(), sim.state_hash())
}
fn press_key(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}
