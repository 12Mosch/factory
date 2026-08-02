//! Load-time checks that a catalog defines the prototypes the engine requires.
//!
//! These cover the recoverable half of base-id resolution: a well-formed data
//! file that simply omits something the engine hard-codes a dependency on is a
//! rejected load with a named cause, not a panic once the id is reached for.

use std::fs;

use crate::base_ids::BasePrototypeIds;
use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;

/// A structurally valid catalog that defines none of the required base names.
const MINIMAL_CATALOG: &str = r#"(
    items: [],
    recipes: [],
    entities: [],
    tiles: [],
)"#;

#[test]
fn base_catalog_defines_every_required_prototype() {
    // `load_base` resolves the required set as part of loading, so a shipped
    // catalog missing one of them fails here with the name it could not find.
    PrototypeCatalog::load_base()
        .expect("the shipped catalog should define every required prototype");
}

#[test]
fn missing_required_prototype_is_reported_by_name() {
    let catalog = PrototypeCatalog::from_ron_str(MINIMAL_CATALOG)
        .expect("a partial catalog should still load structurally");

    let missing = BasePrototypeIds::try_from_catalog(&catalog)
        .expect_err("a catalog with no items cannot resolve the required base ids");

    assert_eq!(missing.group, "item");
    assert_eq!(missing.name, "iron_ore");
    assert_eq!(
        missing.to_string(),
        "missing required item prototype \"iron_ore\""
    );
}

#[test]
fn partial_catalogs_still_load_through_from_ron_str() {
    PrototypeCatalog::from_ron_str(MINIMAL_CATALOG)
        .expect("from_ron_str stays permissive so focused tests can load fragments");
}

#[test]
fn loading_a_playable_catalog_rejects_missing_required_prototypes() {
    let path = std::env::temp_dir().join(format!(
        "factory_data_missing_required_prototype_{}.ron",
        std::process::id()
    ));
    fs::write(&path, MINIMAL_CATALOG).expect("temp catalog should be writable");

    // Clean up before asserting, so a failure cannot leave the file behind.
    let result = PrototypeCatalog::load_from_path(&path);
    let _ = fs::remove_file(&path);

    let error =
        result.expect_err("a catalog missing required prototypes should not load as playable");
    let PrototypeLoadError::MissingRequiredPrototype(missing) = error else {
        panic!("expected a missing-required-prototype error, got {error}");
    };
    assert_eq!(missing.group, "item");
}

#[test]
fn missing_required_prototype_reports_the_group_it_was_looked_up_in() {
    // Items resolve first, so a catalog needs its full item set before a
    // missing tile is the failure the loader reports.
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let missing = crate::try_tile_id_by_name(&catalog, "not_a_tile")
        .expect_err("the base catalog does not define this tile");

    assert_eq!(missing.group, "tile");
    assert_eq!(missing.name, "not_a_tile");
}
