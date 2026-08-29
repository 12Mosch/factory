use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::time::TimeUpdateStrategy;
use factory_app::FactoryAppPlugin;
use factory_app::build::resources::{BuildPlacementState, BuildSelection};
use factory_app::resources::SimResource;
use factory_app::save_load::{
    BACKUP_ARTIFACT_MARKER, PendingSaveConfirmation, PendingSaveJobs, SaveCatalog,
    SaveCompatibility, SaveKind, SaveLoadConfig, SaveLoadMetrics, SaveLoadTab, SaveLoadWindowState,
    TEMP_ARTIFACT_MARKER, decode_container, encode_container, scan_catalog,
};
use factory_app::simulation::SimCommandRequest;
use factory_app::ui::resources::OpenContainer;
use factory_app::ui::save_load::{
    SaveConfirmationButton, SaveCreateButton, SaveEntryAction, SaveEntryButton,
};
use factory_data::{EntityPrototypeId, ItemId};
use factory_sim::{ChunkCoord, EntityId, SAVE_VERSION, SimCommand, load_from_bytes, save_to_bytes};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
        build.selected = Some(BuildSelection::entity(
            EntityPrototypeId::new(0),
            ItemId::new(0),
        ));
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
fn recovery_restores_a_legacy_raw_quicksave_backup() {
    let app = test_app(Duration::ZERO, "raw_quicksave_recovery");
    let expected = sim_tick_and_hash(&app);
    write_raw_quicksave(&app);
    let config = app.world().resource::<SaveLoadConfig>().clone();
    let path = config.root_dir.join("quicksave.factsim");
    let original = fs::read(&path).unwrap();
    let backup = test_artifact_path(&path, BACKUP_ARTIFACT_MARKER, "legacy-writer");
    fs::rename(&path, &backup).unwrap();

    let entries = scan_catalog(&config).unwrap();

    assert_eq!(entries.len(), 1);
    assert!(entries[0].compatibility.can_load());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!backup.exists());
    let loaded = load_from_bytes(&fs::read(&path).unwrap()).unwrap();
    assert_eq!((loaded.tick_count(), loaded.state_hash()), expected);
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

    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app.update();
    create_named_save(&mut app, "MAIN FACTORY");
    assert!(matches!(
        app.world().resource::<PendingSaveConfirmation>(),
        PendingSaveConfirmation::Overwrite(_)
    ));
    press_confirmation(&mut app, true);
    app.update();
    drain_save_jobs(&mut app);
    assert_ne!(fs::read(&path).unwrap(), original);
}

#[test]
fn create_button_uses_editor_text_changed_in_the_same_frame() {
    let mut app = test_app(Duration::ZERO, "same_frame_save_name");
    {
        let mut window = app.world_mut().resource_mut::<SaveLoadWindowState>();
        window.open = true;
        window.tab = SaveLoadTab::Save;
        window.name_buffer = "Previous Name".into();
    }
    app.update();

    let mut found_input = false;
    let mut inputs = app.world_mut().query::<&mut EditableText>();
    for mut input in inputs.iter_mut(app.world_mut()) {
        if input.value() == "Previous Name" {
            input.editor_mut().set_text("Current Name");
            found_input = true;
        }
    }
    assert!(found_input);

    let mut buttons = app
        .world_mut()
        .query_filtered::<&mut Interaction, With<SaveCreateButton>>();
    *buttons.single_mut(app.world_mut()).unwrap() = Interaction::Pressed;
    app.update();
    drain_save_jobs(&mut app);

    assert!(
        app.world()
            .resource::<SaveCatalog>()
            .entries()
            .iter()
            .any(|entry| entry.metadata.display_name == "Current Name")
    );
}

#[test]
fn status_refresh_preserves_the_save_editor_and_same_frame_edit() {
    let mut app = test_app(Duration::ZERO, "stable_save_editor");
    {
        let mut window = app.world_mut().resource_mut::<SaveLoadWindowState>();
        window.open = true;
        window.tab = SaveLoadTab::Save;
        window.name_buffer = "Stable Name".into();
    }
    app.update();

    let mut input_entity = None;
    let mut inputs = app.world_mut().query::<(Entity, &mut EditableText)>();
    for (entity, mut input) in inputs.iter_mut(app.world_mut()) {
        if input.value() == "Stable Name" {
            input.editor_mut().set_text("Stable NameX");
            input_entity = Some(entity);
        }
    }
    let input_entity = input_entity.expect("save-name editor should exist");
    app.world_mut()
        .resource_mut::<factory_app::save_load::SaveLoadStatus>()
        .message = Some("Autosave completed".into());

    app.update();

    let input = app
        .world()
        .entity(input_entity)
        .get::<EditableText>()
        .expect("status refresh must preserve the save-name editor entity");
    assert_eq!(input.value(), "Stable NameX");
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
    assert!(!root.join("manual-x.factsim.tmp-1").exists());
    assert!(!root.join("quicksave.factsim.bak-1").exists());
}

#[test]
fn interrupted_replacement_phases_keep_a_valid_save_discoverable() {
    let mut app = test_app(Duration::ZERO, "interrupted_replacement");
    app.update();
    let old_state = sim_tick_and_hash(&app);
    create_named_save(&mut app, "Crash Test");
    drain_save_jobs(&mut app);
    let config = app.world().resource::<SaveLoadConfig>().clone();
    let path = app.world().resource::<SaveCatalog>().entries()[0]
        .path()
        .to_path_buf();
    let old_bytes = fs::read(&path).unwrap();
    let (mut metadata, _) = decode_container(&old_bytes).unwrap();

    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    run_until_tick(&mut app, old_state.0 + 1);
    freeze_time(&mut app);
    let new_state = sim_tick_and_hash(&app);
    metadata.completed_at_unix_ms = metadata.completed_at_unix_ms.saturating_add(1);
    let new_payload = save_to_bytes(&app.world().resource::<SimResource>().read()).unwrap();
    let new_bytes = encode_container(&metadata, &new_payload).unwrap();

    let temp = test_artifact_path(&path, TEMP_ARTIFACT_MARKER, "4242-0");
    let backup = test_artifact_path(&path, BACKUP_ARTIFACT_MARKER, "4242-0");

    // New contents have been flushed, but installation has not begun.
    fs::write(&temp, &new_bytes).unwrap();
    assert_catalog_loads(&config, old_state);
    assert!(!temp.exists());

    // The rollback copy exists while the intact primary remains canonical.
    fs::copy(&path, &backup).unwrap();
    assert_catalog_loads(&config, old_state);
    assert!(!backup.exists());

    // This is the vulnerable phase used by the previous writer: startup must
    // recover the one fully valid backup, not expose the temporary new file.
    fs::write(&temp, &new_bytes).unwrap();
    fs::copy(&path, &backup).unwrap();
    fs::remove_file(&path).unwrap();
    assert_catalog_loads(&config, old_state);
    assert!(path.is_file());
    assert!(!temp.exists());

    // After atomic installation, the new primary wins over an older backup.
    fs::write(&path, &new_bytes).unwrap();
    fs::write(&backup, &old_bytes).unwrap();
    assert_catalog_loads(&config, new_state);
    assert!(!backup.exists());
}

#[test]
fn recovery_never_replaces_an_intact_primary_or_uses_ambiguous_backups() {
    let mut app = test_app(Duration::ZERO, "safe_recovery_selection");
    app.update();
    let expected = sim_tick_and_hash(&app);
    create_named_save(&mut app, "Recovery Selection");
    drain_save_jobs(&mut app);
    let config = app.world().resource::<SaveLoadConfig>().clone();
    let path = app.world().resource::<SaveCatalog>().entries()[0]
        .path()
        .to_path_buf();
    let valid_bytes = fs::read(&path).unwrap();
    let backup_one = test_artifact_path(&path, BACKUP_ARTIFACT_MARKER, "4242-1");
    let backup_two = test_artifact_path(&path, BACKUP_ARTIFACT_MARKER, "4242-2");

    fs::write(&backup_one, b"invalid backup").unwrap();
    assert_catalog_loads(&config, expected);
    assert_eq!(fs::read(&path).unwrap(), valid_bytes);

    let (metadata, payload) = decode_container(&valid_bytes).unwrap();
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    run_until_tick(&mut app, expected.0 + 1);
    freeze_time(&mut app);
    let different_payload = save_to_bytes(&app.world().resource::<SimResource>().read()).unwrap();
    let different_bytes = encode_container(&metadata, &different_payload).unwrap();
    let mut older_payload = payload.to_vec();
    older_payload[8..12].copy_from_slice(&(SAVE_VERSION - 1).to_le_bytes());
    let incompatible_bytes = encode_container(&metadata, &older_payload).unwrap();
    fs::write(&path, &incompatible_bytes).unwrap();
    fs::write(&backup_one, &valid_bytes).unwrap();
    let entries = scan_catalog(&config).unwrap();
    assert!(matches!(
        entries[0].compatibility,
        SaveCompatibility::SaveFormatOlder { .. }
    ));
    assert_eq!(fs::read(&path).unwrap(), incompatible_bytes);
    assert!(backup_one.exists());

    // An incompatible backup is still intact user data. If it is the only
    // remaining copy, recovery restores it for a compatible game version.
    fs::remove_file(&path).unwrap();
    fs::write(&backup_one, &incompatible_bytes).unwrap();
    let entries = scan_catalog(&config).unwrap();
    assert!(matches!(
        entries[0].compatibility,
        SaveCompatibility::SaveFormatOlder { .. }
    ));
    assert_eq!(fs::read(&path).unwrap(), incompatible_bytes);
    assert!(!backup_one.exists());

    let mut truncated_backup = valid_bytes.clone();
    truncated_backup.truncate(truncated_backup.len() - 8);
    fs::write(&path, b"corrupt primary").unwrap();
    fs::write(&backup_one, truncated_backup).unwrap();
    let entries = scan_catalog(&config).unwrap();
    assert!(!entries[0].compatibility.can_load());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt primary");

    let mut wrong_metadata = metadata.clone();
    wrong_metadata.kind = SaveKind::Quicksave;
    fs::write(
        &backup_one,
        encode_container(&wrong_metadata, payload).unwrap(),
    )
    .unwrap();
    let entries = scan_catalog(&config).unwrap();
    assert!(!entries[0].compatibility.can_load());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt primary");

    fs::write(&backup_one, &valid_bytes).unwrap();
    fs::write(&backup_two, &different_bytes).unwrap();
    let entries = scan_catalog(&config).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].compatibility.can_load());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt primary");

    fs::remove_file(&backup_two).unwrap();
    assert_catalog_loads(&config, expected);
    assert_eq!(fs::read(&path).unwrap(), valid_bytes);
}

#[test]
fn deleting_a_save_removes_recovery_artifacts_without_resurrection() {
    let mut app = test_app(Duration::ZERO, "delete_recovery_artifacts");
    app.update();
    create_named_save(&mut app, "Delete Completely");
    drain_save_jobs(&mut app);
    let entry = app.world().resource::<SaveCatalog>().entries()[0].clone();
    let backup = test_artifact_path(entry.path(), BACKUP_ARTIFACT_MARKER, "delete-test");
    let temporary = test_artifact_path(entry.path(), TEMP_ARTIFACT_MARKER, "delete-test");
    fs::copy(entry.path(), &backup).unwrap();
    fs::copy(entry.path(), &temporary).unwrap();

    press_entry(&mut app, &entry.id, SaveEntryAction::Delete);
    app.update();
    press_confirmation(&mut app, true);
    app.update();

    assert!(!entry.path().exists());
    assert!(!backup.exists());
    assert!(!temporary.exists());
    assert!(app.world().resource::<SaveCatalog>().entries().is_empty());
}

#[test]
fn background_submission_remains_non_blocking_and_metrics_populate() {
    let mut app = test_app(Duration::ZERO, "metrics");
    let captured_tick = app.world().resource::<SimResource>().read().tick_count();
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
    assert_eq!(metrics.last_snapshot_tick, captured_tick);
    assert!(metrics.last_request_submission_ms >= metrics.last_snapshot_capture_ms);
    assert!(metrics.last_total_ms >= metrics.last_write_ms);
}

#[test]
fn commands_around_save_apply_once_and_continue_deterministically() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0), "command_ordering");
    run_until_tick(&mut app, 3);

    let before_save = SimCommand::MovePlayer {
        direction_x: 1.0,
        direction_y: 0.0,
        delta_seconds: 0.25,
    };
    app.world_mut()
        .write_message(SimCommandRequest(before_save));
    press_key(&mut app, KeyCode::F5);
    app.update();
    let snapshot_tick = app.world().resource::<SimResource>().read().tick_count();

    let after_save = SimCommand::MovePlayer {
        direction_x: 0.0,
        direction_y: 1.0,
        delta_seconds: 0.5,
    };
    app.world_mut()
        .write_message(SimCommandRequest(after_save.clone()));
    app.update();
    freeze_time(&mut app);
    assert_eq!(
        app.world().resource::<SimResource>().read().tick_count(),
        snapshot_tick + 1
    );
    let continued_hash = app.world().resource::<SimResource>().read().state_hash();

    drain_save_jobs(&mut app);
    let path = app
        .world()
        .resource::<SaveLoadConfig>()
        .root_dir
        .join("quicksave.factsim");
    let bytes = fs::read(path).unwrap();
    let (_, payload) = decode_container(&bytes).unwrap();
    let mut loaded = load_from_bytes(payload).unwrap();
    assert_eq!(loaded.tick_count(), snapshot_tick);
    loaded.apply_command(&after_save).unwrap();
    loaded.tick();

    assert_eq!(loaded.state_hash(), continued_hash);
}

#[test]
fn large_world_save_measures_capture_and_does_not_block_background_ticks() {
    let mut app = test_app(Duration::ZERO, "large_world_background_save");
    {
        let mut sim_resource = app.world_mut().resource_mut::<SimResource>();
        let mut sim = sim_resource.write_for_tests();
        for y in -10..10 {
            for x in -10..10 {
                sim.ensure_chunk_generated(ChunkCoord { x, y });
            }
        }
    }

    press_key(&mut app, KeyCode::F5);
    let submission_update_started = Instant::now();
    app.update();
    let submission_update = submission_update_started.elapsed();
    assert!(!app.world().resource::<PendingSaveJobs>().is_empty());
    app.world_mut()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));

    let mut measured_ticks = 0;
    let mut max_tick_update = Duration::ZERO;
    for _ in 0..30 {
        if app.world().resource::<PendingSaveJobs>().is_empty() {
            break;
        }
        let before = app.world().resource::<SimResource>().read().tick_count();
        let started = Instant::now();
        app.update();
        max_tick_update = max_tick_update.max(started.elapsed());
        let after = app.world().resource::<SimResource>().read().tick_count();
        if after > before {
            measured_ticks += 1;
            assert_eq!(after, before + 1);
        }
        std::thread::yield_now();
    }

    eprintln!(
        "large-world save: {measured_ticks} fixed ticks observed, max update {:.3} ms",
        max_tick_update.as_secs_f64() * 1000.0
    );
    assert!(measured_ticks > 0);
    assert_eq!(
        app.world()
            .resource::<factory_app::resources::SimProfileStats>()
            .save_blocked_fixed_ticks,
        0
    );
    drain_save_jobs(&mut app);
    let metrics = app.world().resource::<SaveLoadMetrics>();
    eprintln!(
        "large-world snapshot capture: {:.3} ms (submission update {:.3} ms)",
        metrics.last_snapshot_capture_ms,
        submission_update.as_secs_f64() * 1000.0
    );
    assert!(metrics.last_snapshot_capture_ms > 0.0);
    assert!(metrics.last_request_submission_ms >= metrics.last_snapshot_capture_ms);
    assert!(
        submission_update.as_secs_f64() * 1000.0 >= metrics.last_request_submission_ms,
        "the measured submission update must include synchronous snapshot capture"
    );
    assert!(metrics.last_serialize_ms > 0.0);
    assert!(metrics.last_write_ms > 0.0);
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

fn assert_catalog_loads(config: &SaveLoadConfig, expected: (u64, u64)) {
    let entries = scan_catalog(config).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].compatibility.can_load());
    let bytes = fs::read(entries[0].path()).unwrap();
    let (_, payload) = decode_container(&bytes).unwrap();
    let loaded = load_from_bytes(payload).unwrap();
    assert_eq!((loaded.tick_count(), loaded.state_hash()), expected);
}

fn test_artifact_path(path: &std::path::Path, marker: &str, nonce: &str) -> PathBuf {
    path.with_file_name(format!(
        "{}{marker}{nonce}",
        path.file_name().unwrap().to_string_lossy()
    ))
}
