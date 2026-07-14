use glam::IVec2;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::model::{
    ResourceDistanceScalingConfig, ResourceExtraction, TerrainNoiseConfig,
    WORLD_GENERATION_FORMAT_VERSION, WorldGenerationConfig,
};

/// Valid `climate_noise` block shared by the config-fragment tests.
const CLIMATE: &str = "climate_noise: (elevation: (scale: 32, octaves: 3), \
     moisture: (scale: 32, octaves: 3), temperature: (scale: 32, octaves: 3))";
/// Valid single-entry `biomes` block referencing the `grass` test tile.
const BIOMES: &str = "biomes: [(tile: \"grass\", elevation: (min: 0, max: 100), \
     moisture: (min: 0, max: 100), temperature: (min: 0, max: 100))]";

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
    assert!(catalog.world_generation.biomes.is_empty());
    assert!(catalog.world_generation.resources.is_empty());
}

#[test]
fn base_catalog_defines_world_generation() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let config = &catalog.world_generation;

    assert_eq!(config.version, WORLD_GENERATION_FORMAT_VERSION);
    assert_eq!(config.starting_area.min_chunk, -2);
    assert_eq!(config.starting_area.max_chunk, 2);
    assert_eq!(config.biomes.len(), 8);
    assert_eq!(
        config.climate_noise.elevation,
        TerrainNoiseConfig {
            scale: 40,
            octaves: 3,
        }
    );
    // Every biome references a tile that exists in the catalog.
    assert!(
        config
            .biomes
            .iter()
            .all(|biome| catalog.tiles.iter().any(|tile| tile.id == biome.tile))
    );
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
            version: 2,
            starting_area: (min_chunk: -1, max_chunk: 1),
            climate_noise: (
                elevation: (scale: 32, octaves: 3),
                moisture: (scale: 32, octaves: 3),
                temperature: (scale: 32, octaves: 3),
            ),
            biomes: [
                (tile: "water", elevation: (min: 0, max: 30), moisture: (min: 0, max: 100), temperature: (min: 0, max: 100)),
                (tile: "grass", elevation: (min: 30, max: 100), moisture: (min: 0, max: 100), temperature: (min: 0, max: 100)),
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
    assert_eq!(config.biomes[0].tile, catalog.tiles[1].id);
    assert_eq!(config.biomes[1].tile, catalog.tiles[0].id);
    assert_eq!(config.resources[0].resource_item, catalog.items[0].id);
    assert_eq!(config.resources[0].extraction, ResourceExtraction::Solid);
    assert_eq!(
        config.resources[0].starting_patch,
        Some(IVec2::new(-22, -14))
    );
}

#[test]
fn climate_noise_parses_per_channel() {
    let catalog = PrototypeCatalog::from_ron_str(&catalog_ron(
        r#"
        world_generation: Some((
            version: 2,
            starting_area: (min_chunk: 0, max_chunk: 0),
            climate_noise: (
                elevation: (scale: 48, octaves: 4),
                moisture: (scale: 24, octaves: 2),
                temperature: (scale: 64, octaves: 1),
            ),
            biomes: [(tile: "grass", elevation: (min: 0, max: 100), moisture: (min: 0, max: 100), temperature: (min: 0, max: 100))],
            patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3),
        )),
        "#,
    ))
    .expect("catalog should load");
    let noise = catalog.world_generation.climate_noise;
    assert_eq!(
        noise.elevation,
        TerrainNoiseConfig {
            scale: 48,
            octaves: 4,
        }
    );
    assert_eq!(
        noise.moisture,
        TerrainNoiseConfig {
            scale: 24,
            octaves: 2,
        }
    );
    assert_eq!(
        noise.temperature,
        TerrainNoiseConfig {
            scale: 64,
            octaves: 1,
        }
    );
}

#[test]
fn distance_scaling_parses_and_defaults_to_none() {
    let explicit = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
        "world_generation: Some((version: 2, starting_area: (min_chunk: 0, max_chunk: 0), \
         {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), \
         distance_scaling: Some((interval_tiles: 100, richness_bonus_percent: 75, \
         radius_bonus_tiles: 1, max_radius_bonus_tiles: 6)))),"
    )))
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

    let absent = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
        "world_generation: Some((version: 2, starting_area: (min_chunk: 0, max_chunk: 0), \
         {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))),"
    )))
    .expect("catalog should load");
    assert_eq!(absent.world_generation.distance_scaling, None);
}

#[test]
fn unsupported_version_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
        "world_generation: Some((version: 999, starting_area: (min_chunk: 0, max_chunk: 0), \
         {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))),"
    )))
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
fn missing_biome_tile_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
        "world_generation: Some((version: 2, starting_area: (min_chunk: 0, max_chunk: 0), \
         {CLIMATE}, biomes: [(tile: \"lava\", elevation: (min: 0, max: 100), \
         moisture: (min: 0, max: 100), temperature: (min: 0, max: 100))], \
         patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))),"
    )))
    .expect_err("unknown biome tile should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingWorldGenerationTile { tile } if tile == "lava"
    ));
}

#[test]
fn missing_resource_item_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
        "world_generation: Some((version: 2, starting_area: (min_chunk: 0, max_chunk: 0), \
         {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), \
         resources: [(item: \"unobtainium\", extraction: Solid, frequency_percent: 50, \
         radius: 5, richness: 100)])),"
    )))
    .expect_err("unknown resource item should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingWorldGenerationResourceItem { item } if item == "unobtainium"
    ));
}

#[test]
fn duplicate_resource_item_fails() {
    let error = PrototypeCatalog::from_ron_str(&catalog_ron(&format!(
        "world_generation: Some((version: 2, starting_area: (min_chunk: 0, max_chunk: 0), \
         {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), \
         resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 50, radius: 5, richness: 100), \
         (item: \"iron_ore\", extraction: Fluid, frequency_percent: 30, radius: 4, richness: 900)])),"
    )))
    .expect_err("duplicate resource items should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateWorldGenerationResource { item } if item == "iron_ore"
    ));
}

#[test]
fn invalid_numeric_constraints_fail() {
    // Each fragment is a full `world_generation` inner tuple; only the field
    // under test deviates from the valid CLIMATE/BIOMES defaults.
    let cases = [
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 2, max_chunk: -2), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "inverted starting area",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 64), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "starting area above the chunk budget",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 0, jitter: 16, edge_noise: 3))"
            ),
            "zero cell size",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: -1, edge_noise: 3))"
            ),
            "negative jitter",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 4097))"
            ),
            "edge noise above the supported maximum",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 1, jitter: 100, edge_noise: 3))"
            ),
            "resource scan reach above the per-chunk work bound",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, biomes: [], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "empty biomes",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, biomes: [(tile: \"grass\", elevation: (min: 60, max: 40), moisture: (min: 0, max: 100), temperature: (min: 0, max: 100))], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "biome range min not below max",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, biomes: [(tile: \"grass\", elevation: (min: 0, max: 101), moisture: (min: 0, max: 100), temperature: (min: 0, max: 100))], patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "biome range max above 100",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), climate_noise: (elevation: (scale: 0, octaves: 3), moisture: (scale: 32, octaves: 3), temperature: (scale: 32, octaves: 3)), {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "zero climate noise scale",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), climate_noise: (elevation: (scale: 32, octaves: 0), moisture: (scale: 32, octaves: 3), temperature: (scale: 32, octaves: 3)), {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "zero climate noise octaves",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), climate_noise: (elevation: (scale: 32, octaves: 3), moisture: (scale: 32, octaves: 3), temperature: (scale: 32, octaves: 9)), {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3))"
            ),
            "too many climate noise octaves",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), distance_scaling: Some((interval_tiles: 0, richness_bonus_percent: 75, radius_bonus_tiles: 1, max_radius_bonus_tiles: 6)))"
            ),
            "zero distance scaling interval",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), distance_scaling: Some((interval_tiles: 100, richness_bonus_percent: 75, radius_bonus_tiles: 7, max_radius_bonus_tiles: 6)))"
            ),
            "distance scaling radius bonus above its cap",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), distance_scaling: Some((interval_tiles: 100, richness_bonus_percent: 75, radius_bonus_tiles: 1, max_radius_bonus_tiles: 129)))"
            ),
            "distance scaling radius bonus cap above the supported maximum",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), distance_scaling: Some((interval_tiles: 100, richness_bonus_percent: 10001, radius_bonus_tiles: 1, max_radius_bonus_tiles: 6)))"
            ),
            "distance scaling richness bonus above the supported maximum",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 101, radius: 5, richness: 100)])"
            ),
            "frequency above 100",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 50, radius: 0, richness: 100)])"
            ),
            "zero radius",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 50, radius: 16385, richness: 100)])"
            ),
            "resource radius above the supported maximum",
        ),
        (
            format!(
                "(version: 2, starting_area: (min_chunk: 0, max_chunk: 0), {CLIMATE}, {BIOMES}, patch_grid: (cell_size: 40, jitter: 16, edge_noise: 3), resources: [(item: \"iron_ore\", extraction: Solid, frequency_percent: 50, radius: 5, richness: 0)])"
            ),
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
