use super::super::*;

#[test]
fn progress_revision_changes_only_when_visible_progress_changes() {
    let mut progress = OnboardingProgress::default();
    progress.record_electricity_generated();
    assert_eq!(progress.revision, 1);
    progress.record_electricity_generated();
    assert_eq!(progress.revision, 1);

    progress.record_counter(|value| &mut value.labs_placed, 1);
    assert_eq!(progress.revision, 2);
    progress.record_counter(|value| &mut value.labs_placed, 0);
    assert_eq!(progress.revision, 2);

    progress.labs_placed = u64::MAX;
    progress.record_counter(|value| &mut value.labs_placed, 1);
    assert_eq!(progress.revision, 2);
}

#[test]
fn onboarding_progress_survives_v14_save_round_trip() {
    let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
    let mut sim = Simulation::new(73, catalog);
    sim.onboarding_progress = OnboardingProgress {
        revision: 17,
        iron_ore_manually_mined: 10,
        stone_furnaces_placed: 1,
        iron_plates_smelted: 10,
        burner_mining_drills_placed: 1,
        iron_ore_drill_mined: 25,
        transport_belts_manually_crafted: 10,
        electricity_generated: true,
        labs_placed: 1,
        automation_science_packs_produced: 10,
        logistics_researched: true,
        automation_researched: true,
        assembler_items_produced: 1,
        logistic_science_packs_produced: 10,
        oil_processing_researched: true,
        petroleum_gas_produced: 45,
        turrets_researched: true,
        loaded_gun_turrets: 1,
    };

    let bytes = save_to_bytes(&sim).expect("v14 save should serialize");
    let loaded = load_from_bytes(&bytes).expect("v14 save should load");
    assert_eq!(loaded.onboarding_progress(), sim.onboarding_progress());
    assert_eq!(loaded.state_hash(), sim.state_hash());
}

#[test]
fn v13_save_header_is_rejected() {
    let catalog = PrototypeCatalog::load_base().expect("base prototypes should load");
    let mut bytes = save_to_bytes(&Simulation::new(91, catalog)).expect("save should serialize");
    bytes[8..12].copy_from_slice(&13_u32.to_le_bytes());
    assert!(matches!(
        load_from_bytes(&bytes),
        Err(SaveLoadError::UnsupportedSaveVersion {
            found: 13,
            supported: SAVE_VERSION
        })
    ));
}
