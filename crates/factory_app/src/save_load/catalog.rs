pub(crate) mod inspect;
pub(crate) mod polling;
pub(crate) mod recovery;
pub(crate) mod scan;
pub(crate) mod validation;

pub(crate) use inspect::now_unix_ms;
pub(crate) use polling::poll_catalog_validation_jobs;
pub use scan::{refresh_catalog, refresh_catalog_blocking, scan_catalog};

#[cfg(test)]
use super::container::{ContainerError, fallback_metadata, load_simulation_from_reader};
#[cfg(test)]
use super::{
    CachedSaveValidation, CatalogValidationJob, CatalogValidationOutcome, CatalogValidationRequest,
    SaveCatalog, SaveCompatibility, SaveEntry, SaveFileMetadataFingerprint, SaveId, SaveKind,
    SaveLoadConfig,
};
#[cfg(test)]
use factory_sim::{SaveLimits, load_from_bytes};
#[cfg(test)]
pub(crate) use inspect::{
    inspect_file, recognized_file, save_file_fingerprint, save_file_metadata_fingerprint,
};
#[cfg(test)]
pub(crate) use polling::poll_catalog_validation_jobs_inner;
#[cfg(test)]
pub(crate) use recovery::{
    PrimaryState, RecoveryBackup, classify_backup_result, classify_simulation_result,
};
#[cfg(test)]
pub(crate) use scan::{inspect_entry, prepare_entry_validation};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::io::Read as _;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[cfg(test)]
use std::thread;
#[cfg(test)]
use std::time::{Duration, Instant};
#[cfg(test)]
pub(crate) use validation::{
    CancelReader, MAX_CATALOG_VALIDATION_RETRIES, validate_loadable_file, validate_loadable_path,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_refresh_defers_payload_validation_to_a_bounded_worker() {
        let root = std::env::temp_dir().join(format!(
            "factory-background-validation-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("quicksave.factsim"),
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim"),
        )
        .unwrap();
        let config = SaveLoadConfig {
            root_dir: root.clone(),
            autosave_interval_ticks: 300,
            autosave_slot_count: 5,
        };
        let mut catalog = SaveCatalog::default();

        refresh_catalog(&config, &mut catalog).unwrap();

        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::ValidationPending
        );
        assert_eq!(catalog.validation_jobs.len(), 1);
        assert!(catalog.validation_queue.is_empty());
        drop(catalog);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stable_file_identity_invalidates_same_size_timestamp_cache_entry() {
        let path = std::env::temp_dir().join(format!(
            "factory-migration-cache-{}-{}.factsim",
            std::process::id(),
            now_unix_ms()
        ));
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let mut file = fs::File::open(&path).unwrap();
        let metadata = save_file_metadata_fingerprint(&file);
        let current_fingerprint = save_file_fingerprint(&mut file, metadata).unwrap();
        let mut stale_fingerprint = current_fingerprint.clone();
        stale_fingerprint
            .metadata
            .identity
            .as_mut()
            .expect("test filesystem should expose stable file identity")
            .change_time ^= 1;
        let mut cache = BTreeMap::from([(
            path.clone(),
            CachedSaveValidation {
                fingerprint: stale_fingerprint,
                compatibility: SaveCompatibility::CorruptOrTruncated,
            },
        )]);
        let header_compatibility = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let compatibility =
            validate_loadable_file(&path, &SaveKind::Quicksave, current_hash, &mut cache);

        assert_eq!(compatibility, header_compatibility);
        assert_eq!(cache[&path].fingerprint, current_fingerprint);
        assert_eq!(cache[&path].compatibility, header_compatibility);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn oversized_migration_candidate_is_rejected_before_hashing() {
        let path = std::env::temp_dir().join(format!(
            "factory-oversized-migration-{}-{}.factsim",
            std::process::id(),
            now_unix_ms()
        ));
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(SaveLimits::default().max_encoded_bytes + 1)
            .unwrap();
        drop(file);
        let mut cache = BTreeMap::new();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let compatibility =
            validate_loadable_file(&path, &SaveKind::Quicksave, current_hash, &mut cache);

        assert_eq!(compatibility, SaveCompatibility::ExceedsCurrentLimits);
        assert!(cache.is_empty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn policy_rejection_preserves_primary_and_defers_backup_recovery() {
        let sim = factory_sim::Simulation::new_test_world(292);
        let bytes = factory_sim::save_to_bytes(&sim).unwrap();
        let limits = factory_sim::SaveLimits {
            max_decoded_bytes: 1,
            ..Default::default()
        };
        let rejected = || factory_sim::load_from_bytes_with_limits(&bytes, limits);
        assert_eq!(
            classify_simulation_result(rejected()),
            PrimaryState::IntactButIncompatible
        );
        assert!(matches!(
            classify_backup_result(bytes.clone(), rejected()),
            RecoveryBackup::Inaccessible(ContainerError::TooLarge)
        ));
        assert_eq!(
            classify_simulation_result(load_from_bytes(&bytes)),
            PrimaryState::Valid
        );
        assert!(matches!(
            classify_backup_result(bytes.clone(), load_from_bytes(&bytes)),
            RecoveryBackup::Candidate(_)
        ));
        assert_eq!(
            classify_simulation_result(load_from_bytes(&[0])),
            PrimaryState::Corrupt
        );
        assert!(matches!(
            classify_backup_result(vec![0], load_from_bytes(&[0])),
            RecoveryBackup::Corrupt
        ));
    }

    #[test]
    fn stale_retry_preserves_queued_current_header_classification() {
        use std::collections::VecDeque;

        let dir = std::env::temp_dir().join(format!(
            "factory-stale-retry-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let mut open_a = fs::File::open(&path).unwrap();
        let metadata_a = save_file_metadata_fingerprint(&open_a);
        let fingerprint_a = save_file_fingerprint(&mut open_a, metadata_a.clone()).unwrap();
        drop(open_a);

        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata_b = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_ne!(
            metadata_a, metadata_b,
            "replacement should change file identity for the test"
        );
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());
        assert_eq!(
            inspect_file(&path, &SaveKind::Quicksave, current_hash).compatibility,
            SaveCompatibility::Compatible,
            "replacement bytes should classify as current format"
        );

        let migratable = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };
        let stale = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: migratable.clone(),
            observed_metadata: Some(metadata_a.clone()),
            fingerprint: Some(fingerprint_a),
            attempt: 0,
        };
        // Simulate the old worker finishing after the replacement + refresh.
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: crate::save_load::SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    crate::save_load::SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: VecDeque::from([CatalogValidationRequest {
                path: path.clone(),
                kind: SaveKind::Quicksave,
                compatibility: SaveCompatibility::Compatible,
                metadata: metadata_b.clone(),
                attempt: 0,
            }]),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: metadata_a,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| stale),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible,
            "stale v57 retry must not publish Migratable for the replaced v58 file"
        );
        assert_eq!(
            catalog
                .validation_cache
                .get(&path)
                .map(|cached| cached.compatibility.clone()),
            Some(SaveCompatibility::Compatible)
        );
        assert_eq!(
            catalog.validation_cache[&path].fingerprint.metadata,
            metadata_b
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_retry_recomputes_current_header_without_queued_request() {
        let dir = std::env::temp_dir().join(format!(
            "factory-stale-retry-fresh-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let mut open_a = fs::File::open(&path).unwrap();
        let metadata_a = save_file_metadata_fingerprint(&open_a);
        let fingerprint_a = save_file_fingerprint(&mut open_a, metadata_a.clone()).unwrap();
        drop(open_a);

        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata_b = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());

        let migratable = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };
        let stale = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: migratable,
            observed_metadata: Some(metadata_a.clone()),
            fingerprint: Some(fingerprint_a),
            attempt: 0,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: crate::save_load::SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    crate::save_load::SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: metadata_a,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| stale),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible,
            "retry after replacement must use the current header, not the stale source"
        );
        assert_eq!(
            catalog.validation_cache[&path].fingerprint.metadata,
            metadata_b
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replaced_file_between_inspection_and_request_uses_current_header() {
        let dir = std::env::temp_dir().join(format!(
            "factory-inspection-replacement-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        fs::write(&path, historical).unwrap();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        // Header inspection observes the v57 file.
        let mut entry = inspect_entry(
            path.clone(),
            SaveId::new("quicksave"),
            SaveKind::Quicksave,
            "Quicksave".into(),
            current_hash,
        );
        assert!(matches!(
            entry.compatibility,
            SaveCompatibility::MigratableSaveFormat { .. }
        ));

        // The path is replaced by a current-format save before the validation
        // request is created.
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();

        let cache = BTreeMap::new();
        let mut requests = Vec::new();
        prepare_entry_validation(&mut entry, current_hash, &cache, &mut requests);

        assert_eq!(entry.compatibility, SaveCompatibility::ValidationPending);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].compatibility, SaveCompatibility::Compatible);
        assert_eq!(requests[0].kind, SaveKind::Quicksave);
        let current_metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_eq!(requests[0].metadata, current_metadata);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn worker_replacement_after_request_uses_current_header() {
        let dir = std::env::temp_dir().join(format!(
            "factory-worker-replacement-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let historical =
            include_bytes!("../../../factory_sim/tests/fixtures/save-v57-sanitized.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        let migratable = SaveCompatibility::MigratableSaveFormat {
            found: factory_sim::OLDEST_SUPPORTED_SAVE_VERSION,
            current: factory_sim::SAVE_VERSION,
        };

        // The request carries a current-format classification, but the path
        // now resolves to v57 bytes.
        fs::write(&path, &current).unwrap();
        let metadata_current = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        fs::write(&path, historical).unwrap();
        let outcome = validate_loadable_path(
            path.clone(),
            SaveKind::Quicksave,
            SaveCompatibility::Compatible,
            Some(metadata_current),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(outcome.compatibility, migratable);
        let validated_metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        assert_eq!(
            outcome.fingerprint.map(|fingerprint| fingerprint.metadata),
            Some(validated_metadata)
        );

        // And the reverse: a stale migratable classification must not be
        // published for current bytes.
        let metadata_historical = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        fs::write(&path, &current).unwrap();
        let outcome = validate_loadable_path(
            path.clone(),
            SaveKind::Quicksave,
            migratable,
            Some(metadata_historical),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        assert_eq!(outcome.compatibility, SaveCompatibility::Compatible);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn second_transient_failure_schedules_bounded_retry() {
        let dir = std::env::temp_dir().join(format!(
            "factory-transient-retry-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());

        // The first validation and its retry both hit transient I/O
        // failures. Loading must not stay disabled.
        let transient = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::ValidationPending,
            observed_metadata: Some(metadata),
            fingerprint: None,
            attempt: 1,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: save_file_metadata_fingerprint(&fs::File::open(&path).unwrap()),
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| transient),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_file_schedules_bounded_blind_retry() {
        let dir = std::env::temp_dir().join(format!(
            "factory-blind-retry-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata_before = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        fs::remove_file(&path).unwrap();

        let transient = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::ValidationPending,
            observed_metadata: Some(metadata_before.clone()),
            fingerprint: None,
            attempt: 0,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: metadata_before,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| transient),
            }],
        };

        // Wait until the blind retry is in flight: it carries a placeholder
        // identity because the file could not be opened for fingerprinting.
        let placeholder = SaveFileMetadataFingerprint {
            len: 0,
            modified: None,
            identity: None,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while catalog.validation_jobs.len() != 1
            || catalog.validation_jobs[0].metadata != placeholder
        {
            assert!(
                std::time::Instant::now() < deadline,
                "blind retry was not scheduled"
            );
            poll_catalog_validation_jobs_inner(&mut catalog);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        // The file reappears while the blind retry is outstanding. Either the
        // running retry or its bounded successor must observe and publish it.
        fs::write(&path, &current).unwrap();
        drain_validation_jobs(&mut catalog);

        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        assert!(catalog.validation_cache[&path].fingerprint.metadata.len > 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retries_stop_at_attempt_bound() {
        let dir = std::env::temp_dir().join(format!(
            "factory-retry-bound-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        // The file is valid and present, but the attempt budget is spent:
        // no further validation may be scheduled by the chain. The periodic
        // rescan (disabled here) owns any later observation.
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let metadata = save_file_metadata_fingerprint(&fs::File::open(&path).unwrap());
        let transient = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::ValidationPending,
            observed_metadata: Some(metadata),
            fingerprint: None,
            attempt: MAX_CATALOG_VALIDATION_RETRIES,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: SaveFileMetadataFingerprint {
                    len: 0,
                    modified: None,
                    identity: None,
                },
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| transient),
            }],
        };

        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::ValidationPending
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepare_open_failure_queues_blind_retry() {
        let dir = std::env::temp_dir().join(format!(
            "factory-prepare-blind-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        // The file is momentarily unreadable: no bytes were ever observed.
        let path = dir.join("quicksave.factsim");
        let mut entry = SaveEntry {
            id: SaveId::new("quicksave"),
            metadata: fallback_metadata(
                SaveId::new("quicksave"),
                SaveKind::Quicksave,
                "Quicksave".into(),
                0,
            ),
            compatibility: SaveCompatibility::Compatible,
            metadata_available: true,
            path: path.clone(),
            inspected: None,
        };
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let cache = BTreeMap::new();
        let mut requests = Vec::new();
        prepare_entry_validation(&mut entry, current_hash, &cache, &mut requests);

        assert_eq!(entry.compatibility, SaveCompatibility::ValidationPending);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attempt, 0);
        assert_eq!(
            requests[0].compatibility,
            SaveCompatibility::ValidationPending
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exhausted_pending_entry_is_rescanned() {
        let dir = std::env::temp_dir().join(format!(
            "factory-exhausted-rescan-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();

        // Retries are exhausted and no validation is scheduled, but the file
        // is valid: the periodic rescan must observe and publish it.
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: None,
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: Vec::new(),
        };

        // The drain helper only polls while jobs exist; prime one poll so
        // the rescan (which is what schedules work here) can run.
        poll_catalog_validation_jobs_inner(&mut catalog);
        drain_validation_jobs(&mut catalog);

        assert_eq!(catalog.validation_jobs.len(), 0);
        assert!(catalog.validation_queue.is_empty());
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        assert!(catalog.next_pending_rescan.is_some());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancel_reader_aborts_reads_when_set() {
        let bytes = b"FACTSIM payload";
        let cancel = Arc::new(AtomicBool::new(false));
        let mut reader = CancelReader {
            reader: std::io::Cursor::new(&bytes[..]),
            cancel: cancel.clone(),
        };
        let mut first = [0; 7];
        reader.read_exact(&mut first).unwrap();
        assert_eq!(&first, b"FACTSIM");
        cancel.store(true, Ordering::Relaxed);
        assert!(reader.read(&mut first).is_err());
    }

    #[test]
    fn cancelled_decode_aborts_without_consuming_payload() {
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        let cancel = Arc::new(AtomicBool::new(true));
        let mut reader = CancelReader {
            reader: std::io::Cursor::new(&current),
            cancel,
        };
        let result = load_simulation_from_reader(&mut reader, factory_sim::SaveLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn drop_signals_running_jobs_before_joining() {
        let dir = std::env::temp_dir().join(format!(
            "factory-drop-cancel-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let catalog = SaveCatalog {
            entries: Vec::new(),
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata: SaveFileMetadataFingerprint {
                    len: 0,
                    modified: None,
                    identity: None,
                },
                cancel,
                handle: thread::spawn(move || {
                    let outcome = CatalogValidationOutcome {
                        path,
                        compatibility: SaveCompatibility::ValidationPending,
                        observed_metadata: None,
                        fingerprint: None,
                        attempt: 0,
                    };
                    let mut spins = 0u32;
                    loop {
                        if worker_cancel.load(Ordering::Relaxed) {
                            let _ = done_tx.send(true);
                            return outcome;
                        }
                        spins += 1;
                        if spins > 2_000 {
                            let _ = done_tx.send(false);
                            return outcome;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }),
            }],
        };

        drop(catalog);

        // Fails (after ~2s) if drop joins without signalling first.
        assert_eq!(
            done_rx.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(true)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn inspection_io_failure_is_pending_not_corrupt() {
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());
        let missing = std::env::temp_dir().join(format!(
            "factory-inspect-missing-{}-{}.factsim",
            std::process::id(),
            now_unix_ms()
        ));
        let _ = fs::remove_file(&missing);

        let inspection = inspect_file(&missing, &SaveKind::Quicksave, current_hash);
        assert_eq!(
            inspection.compatibility,
            SaveCompatibility::ValidationPending
        );
        assert!(!inspection.safe_to_replace);
        assert!(inspection.inspected.is_none());
    }

    #[test]
    fn inspection_short_file_is_corrupt() {
        let dir = std::env::temp_dir().join(format!(
            "factory-inspect-short-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        fs::write(&path, b"short").unwrap();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        let inspection = inspect_file(&path, &SaveKind::Quicksave, current_hash);
        assert_eq!(
            inspection.compatibility,
            SaveCompatibility::CorruptOrTruncated
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepare_pending_entry_with_readable_file_queues_validation() {
        let dir = std::env::temp_dir().join(format!(
            "factory-prepare-pending-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let current_hash =
            factory_sim::prototype_hash(&factory_data::PrototypeCatalog::load_base().unwrap());

        // Inspection failed transiently, so no classification exists, but the
        // file is readable now.
        let mut entry = SaveEntry {
            id: SaveId::new("quicksave"),
            metadata: fallback_metadata(
                SaveId::new("quicksave"),
                SaveKind::Quicksave,
                "Quicksave".into(),
                0,
            ),
            compatibility: SaveCompatibility::ValidationPending,
            metadata_available: true,
            path: path.clone(),
            inspected: None,
        };
        let cache = BTreeMap::new();
        let mut requests = Vec::new();
        prepare_entry_validation(&mut entry, current_hash, &cache, &mut requests);

        assert_eq!(entry.compatibility, SaveCompatibility::ValidationPending);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].compatibility, SaveCompatibility::Compatible);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn idle_poll_reports_no_mutation() {
        let dir = std::env::temp_dir().join(format!(
            "factory-idle-poll-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::Compatible,
                metadata_available: true,
                path,
                inspected: None,
            }],
            revision: 7,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: Vec::new(),
        };

        assert!(!poll_catalog_validation_jobs_inner(&mut catalog));
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        assert_eq!(catalog.revision, 7);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finished_outcome_reports_mutation() {
        let dir = std::env::temp_dir().join(format!(
            "factory-outcome-poll-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let current = factory_sim::save_to_bytes(&factory_sim::Simulation::new_test_world(7))
            .expect("current save should encode");
        fs::write(&path, &current).unwrap();
        let mut open = fs::File::open(&path).unwrap();
        let metadata = save_file_metadata_fingerprint(&open);
        let fingerprint = save_file_fingerprint(&mut open, metadata.clone()).unwrap();
        drop(open);

        let outcome = CatalogValidationOutcome {
            path: path.clone(),
            compatibility: SaveCompatibility::Compatible,
            observed_metadata: Some(metadata.clone()),
            fingerprint: Some(fingerprint),
            attempt: 0,
        };
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::ValidationPending,
                metadata_available: true,
                path: path.clone(),
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: Some(Instant::now() + Duration::from_secs(3600)),
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: vec![CatalogValidationJob {
                path: path.clone(),
                metadata,
                cancel: Arc::new(AtomicBool::new(false)),
                handle: thread::spawn(|| outcome),
            }],
        };

        // A published outcome schedules no retry; the loop ends once the
        // single job is consumed.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut mutated = false;
        while !catalog.validation_jobs.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "validation job did not finish"
            );
            mutated |= poll_catalog_validation_jobs_inner(&mut catalog);
            if !catalog.validation_jobs.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        assert!(mutated);
        assert_eq!(
            catalog.entries[0].compatibility,
            SaveCompatibility::Compatible
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rescan_throttle_advance_reports_no_mutation() {
        let dir = std::env::temp_dir().join(format!(
            "factory-throttle-poll-{}-{}",
            std::process::id(),
            now_unix_ms()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("quicksave.factsim");
        let mut catalog = SaveCatalog {
            entries: vec![SaveEntry {
                id: SaveId::new("quicksave"),
                metadata: fallback_metadata(
                    SaveId::new("quicksave"),
                    SaveKind::Quicksave,
                    "Quicksave".into(),
                    0,
                ),
                compatibility: SaveCompatibility::Compatible,
                metadata_available: true,
                path,
                inspected: None,
            }],
            revision: 0,
            validation_cache: BTreeMap::new(),
            next_pending_rescan: None,
            validation_queue: std::collections::VecDeque::new(),
            validation_jobs: Vec::new(),
        };

        // The throttle advances but nothing observable changes.
        assert!(!poll_catalog_validation_jobs_inner(&mut catalog));
        assert!(catalog.next_pending_rescan.is_some());
        fs::remove_dir_all(dir).unwrap();
    }

    fn drain_validation_jobs(catalog: &mut SaveCatalog) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !catalog.validation_jobs.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "validation job did not finish"
            );
            poll_catalog_validation_jobs_inner(catalog);
            if !catalog.validation_jobs.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn old_and_unrelated_file_names_are_ignored() {
        let count = 5;
        for name in [
            "slot_1.factsim",
            "slot_2.factsim",
            "slot_3.factsim",
            "autosave.factsim",
            "quicksave.factsim.tmp-1",
            "file.txt",
        ] {
            assert!(recognized_file(Path::new(name), count).is_none());
        }
        assert!(recognized_file(Path::new("manual-abc.factsim"), count).is_some());
        assert!(recognized_file(Path::new("autosave-5.factsim"), count).is_some());
    }
}
