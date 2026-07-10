use glam::IVec2;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::model::{
    ResourceDistanceScalingConfig, ResourceExtraction, TerrainNoiseConfig,
    WORLD_GENERATION_FORMAT_VERSION, WorldGenerationConfig,
};

fn catalog_ron(world_generation: &str) -> String {
    format!(
        r#"
        (
            items: [
                (id: 0, name: "iron_ore", stack_size: 100),
            ],
            recipes: [],
            entities: [],
            tiles: [
                (id: 0, name: "grass", collision_mask: (layers: ["ground"])),
                (id: 1, name: "water", collision_mask: (layers: ["water"])),
            ],
            {world_generation}
        )
        "#
    )
}

#[test]
fn missing_section_defaults_to_empty_config() {
    let catalog = PrototypeCatalog::from_ron_str(&catalog_ron("")).expect("catalog should load");

    assert_eq!(catalog.world_generation, WorldGenerationConfig::default());
    assert!(catalog.world_generation.terrain.is_empty());
    assert!(catalog.world_generation.resources.is_empty());
}

#[test]
fn base_catalog_defines_world_generation() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let config = &catalog.world_generation;

    assert_eq!(config.version, WORLD_GENERATION_FORMAT_VERSION);
    assert_eq!(config.starting_area.min_chunk, -2);
    assert_eq!(config.starting_area.max_chunk, 2);
    assert_eq!(config.terrain.len(), 3);
    assert_eq!(
        config.terrain.iter().map(|layer| layer.weight).sum::<u32>(),
        100
    );
    assert_eq!(config.terrain_noise, TerrainNoiseConfig::default());
    assert_eq!(config.patch_grid.cell_size, 40);
    assert!(config.distance_scaling.is_some());
    assert_eq!(config.resources.len(), 5);
    assert_eq!(
        config
            .resources
            .iter()
            .filter(|resource| resource.extraction == ResourceExtraction::Fluid)
            .count(),
        1
    );
    assert!(
        config
            .resources
            .iter()
            .all(|resource| resource.starting_patch.is_some())
    );
}

#[test]
fn section_resolves_names_to_ids() {
    let catalog = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: -1, max_chunk: 1),
            terrain: [
                (tile: "water", weight: 10),
                (tile: "grass", weight: 90),
            ],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
            resources: [
                (
                    item: "iron_ore",
                    extraction: Solid,
                    frequency_percent: 68,
                    radius: 9,
                    richness: 700,
                    starting_patch: Some((x: -22, y: -14)),
                ),
            ],
        )),
        "#,
    ))
    .expect("catalog should load");

    let config = &catalog.world_generation;
    assert_eq!(config.terrain[0].tile, catalog.tiles[1].id);
    assert_eq!(config.terrain[1].tile, catalog.tiles[0].id);
    assert_eq!(config.resources[0].resource_item, catalog.items[0].id);
    assert_eq!(config.resources[0].extraction, ResourceExtraction::Solid);
    assert_eq!(
        config.resources[0].starting_patch,
        Some(IVec2::new(-22, -14))
    );
}

#[test]
fn terrain_noise_parses_and_defaults_when_absent() {
    let explicit = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            terrain_noise: Some((scale: 48, octaves: 4)),
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
        )),
        "#,
    ))
    .expect("catalog should load");
    assert_eq!(
        explicit.world_generation.terrain_noise,
        TerrainNoiseConfig {
            scale: 48,
            octaves: 4,
        }
    );

    let absent = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
        )),
        "#,
    ))
    .expect("catalog should load");
    assert_eq!(
        absent.world_generation.terrain_noise,
        TerrainNoiseConfig::default()
    );
}

#[test]
fn distance_scaling_parses_and_defaults_to_none() {
    let explicit = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
            distance_scaling: Some((
                interval_tiles: 100,
                richness_bonus_percent: 75,
                radius_bonus_tiles: 1,
                max_radius_bonus_tiles: 6,
            )),
        )),
        "#,
    ))
    .expect("catalog should load");
    assert_eq!(
        explicit.world_generation.distance_scaling,
        Some(ResourceDistanceScalingConfig {
            interval_tiles: 100,
            richness_bonus_percent: 75,
            radius_bonus_tiles: 1,
            max_radius_bonus_tiles: 6,
        })
    );

    let absent = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
        )),
        "#,
    ))
    .expect("catalog should load");
    assert_eq!(absent.world_generation.distance_scaling, None);
}

#[test]
fn unsupported_version_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 999,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
        )),
        "#,
    ))
    .expect_err("unsupported version should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::UnsupportedWorldGenerationVersion {
            found: 999,
            supported: WORLD_GENERATION_FORMAT_VERSION,
        }
    ));
}

#[test]
fn missing_terrain_tile_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "lava", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
        )),
        "#,
    ))
    .expect_err("unknown terrain tile should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingWorldGenerationTile { tile } if tile == "lava"
    ));
}

#[test]
fn missing_resource_item_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
            resources: [
                (
                    item: "unobtainium",
                    extraction: Solid,
                    frequency_percent: 50,
                    radius: 5,
                    richness: 100,
                ),
            ],
        )),
        "#,
    ))
    .expect_err("unknown resource item should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingWorldGenerationResourceItem { item } if item == "unobtainium"
    ));
}

#[test]
fn duplicate_resource_item_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 1,
            starting_area: (min_chunk: 0, max_chunk: 0),
            terrain: [(tile: "grass", weight: 1)],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
            resources: [
                (item: "iron_ore", extraction: Solid, frequency_percent: 50, radius: 5, richness: 100),
                (item: "iron_ore", extraction: Fluid, frequency_percent: 30, radius: 4, richness: 900),
            ],
        )),
        "#,
    ))
    .expect_err("duplicate resource items should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateWorldGenerationResource { item } if item == "iron_ore"
    ));
}

#[test]
fn invalid_numeric_constraints_fail() {
    let cases = [
        (
            "(version: 1, starting_area: (min_chunk: 2, max_chunk: -2), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))",
            "inverted starting area",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 0, jitter: 16, edge_noise: 3))",
            "zero cell size",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: -1, edge_noise: 3))",
            "negative jitter",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))",
            "empty terrain",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 0)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))",
            "all-zero terrain weights",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], terrain_noise: Some((scale: 0, octaves: 3)), patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))",
            "zero noise scale",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], terrain_noise: Some((scale: 32, octaves: 0)), patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))",
            "zero noise octaves",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], terrain_noise: Some((scale: 32, octaves: 9)), patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))",
            "too many noise octaves",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), distance_scaling: Some((interval_tiles: 0, richness_bonus_percent: 75, radius_bonus_tiles: 1, max_radius_bonus_tiles: 6)))",
            "zero distance scaling interval",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), distance_scaling: Some((interval_tiles: 100, richness_bonus_percent: 75, radius_bonus_tiles: 7, max_radius_bonus_tiles: 6)))",
            "distance scaling radius bonus above its cap",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 101, radius: 5, richness: 100)])",
            "frequency above 100",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 50, radius: 0, richness: 100)])",
            "zero radius",
        ),
        (
            "(version: 1, starting_area: (min_chunk: 0, max_chunk: 0), terrain: [(tile: \"grass\", weight: 1)], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 50, radius: 5, richness: 0)])",
            "zero richness",
        ),
    ];

    for (section, case) in cases {
        let error = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
            "world_generation: Some({section}),"
        )))
        .expect_err(case);

        assert!(
            matches!(
                error,
                PrototypeLoadError::InvalidWorldGenerationConfig { .. }
            ),
            "expected InvalidWorldGenerationConfig for {case}, got {error:?}"
        );
    }
}
