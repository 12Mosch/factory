use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::{DayNightCycleConfig, PrototypeCatalog, PrototypeLoadError};

fn catalog_ron(day_night_cycle: &str) -> String {
    format!(
        r#"(
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            {day_night_cycle}
        )"#
    )
}

fn catalog_hash(catalog: &PrototypeCatalog) -> u64 {
    let mut hasher = DefaultHasher::new();
    catalog.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn base_catalog_exposes_configured_cycle() {
    let catalog = PrototypeCatalog::load_base().expect("base catalog should load");

    assert_eq!(
        catalog.day_night_cycle,
        Some(DayNightCycleConfig {
            cycle_length_ticks: 25_000,
            dawn_dusk_ticks: 5_000,
        })
    );
}

#[test]
fn missing_cycle_is_valid_and_stays_disabled() {
    let catalog = PrototypeCatalog::from_ron_str(&catalog_ron(""))
        .expect("catalog should load without cycle");

    assert_eq!(catalog.day_night_cycle, None);
}

#[test]
fn invalid_cycle_timings_are_rejected() {
    let invalid_configs = [
        "(cycle_length_ticks: 0, dawn_dusk_ticks: 1)",
        "(cycle_length_ticks: 10, dawn_dusk_ticks: 0)",
        "(cycle_length_ticks: 18446744073709551615, dawn_dusk_ticks: 18446744073709551615)",
        "(cycle_length_ticks: 40, dawn_dusk_ticks: 10)",
    ];

    for config in invalid_configs {
        let error = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
            "day_night_cycle: Some({config}),"
        )))
        .expect_err("invalid cycle timing should fail");

        assert!(
            matches!(error, PrototypeLoadError::InvalidDayNightCycleConfig),
            "unexpected error for {config}: {error:?}"
        );
    }
}

#[test]
fn day_night_timing_participates_in_catalog_hash() {
    let first = PrototypeCatalog::from_ron_str(&catalog_ron(
        "day_night_cycle: Some((cycle_length_ticks: 100, dawn_dusk_ticks: 20)),",
    ))
    .expect("first catalog should load");
    let second = PrototypeCatalog::from_ron_str(&catalog_ron(
        "day_night_cycle: Some((cycle_length_ticks: 101, dawn_dusk_ticks: 20)),",
    ))
    .expect("second catalog should load");

    assert_ne!(catalog_hash(&first), catalog_hash(&second));
}
