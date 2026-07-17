use crate::catalog::PrototypeCatalog;

#[test]
fn base_enemy_gameplay_values_are_valid_and_data_driven() {
    let catalog = PrototypeCatalog::load_base().unwrap();
    let enemy = catalog
        .enemy_gameplay
        .expect("base catalog has enemy gameplay");
    assert_eq!(
        (
            enemy.generated_colony_min_spawners,
            enemy.generated_colony_max_spawners
        ),
        (2, 4)
    );
    assert_eq!(enemy.raid_cooldown_ticks, 7_200);
    assert_eq!(enemy.expansion_interval_ticks, 36_000);
    assert_eq!(enemy.expansion_candidate_limit, 128);
}

const ITEM_NAMES: [&str; 42] = [
    "iron_ore",
    "copper_ore",
    "coal",
    "stone",
    "iron_plate",
    "copper_plate",
    "steel_plate",
    "iron_gear_wheel",
    "copper_cable",
    "electronic_circuit",
    "inserter",
    "transport_belt",
    "assembling_machine",
    "stone_furnace",
    "burner_mining_drill",
    "lab",
    "automation_science_pack",
    "chest",
    "stone_brick",
    "underground_belt",
    "splitter",
    "fast_transport_belt",
    "express_transport_belt",
    "fast_underground_belt",
    "express_underground_belt",
    "fast_splitter",
    "express_splitter",
    "fast_inserter",
    "long_handed_inserter",
    "small_electric_pole",
    "steam_engine",
    "boiler",
    "offshore_pump",
    "pipe",
    "storage_tank",
    "logistic_science_pack",
    "crude_oil",
    "pumpjack",
    "oil_refinery",
    "chemical_plant",
    "plastic_bar",
    "sulfur",
];

const FLUID_NAMES: [&str; 4] = ["water", "steam", "crude_oil", "petroleum_gas"];

const RECIPE_NAMES: [&str; 38] = [
    "iron_plate",
    "copper_plate",
    "steel_plate",
    "iron_gear_wheel",
    "copper_cable",
    "electronic_circuit",
    "inserter",
    "transport_belt",
    "assembling_machine",
    "stone_furnace",
    "burner_mining_drill",
    "lab",
    "automation_science_pack",
    "chest",
    "stone_brick",
    "underground_belt",
    "splitter",
    "fast_inserter",
    "long_handed_inserter",
    "small_electric_pole",
    "steam_engine",
    "boiler",
    "offshore_pump",
    "pipe",
    "storage_tank",
    "logistic_science_pack",
    "fast_transport_belt",
    "fast_underground_belt",
    "fast_splitter",
    "express_transport_belt",
    "express_underground_belt",
    "express_splitter",
    "pumpjack",
    "oil_refinery",
    "chemical_plant",
    "basic_oil_processing",
    "plastic_bar",
    "sulfur",
];

const ENTITY_NAMES: [&str; 34] = [
    "iron_ore_patch",
    "copper_ore_patch",
    "coal_patch",
    "stone_patch",
    "stone_furnace",
    "assembling_machine",
    "inserter",
    "transport_belt",
    "burner_mining_drill",
    "lab",
    "chest",
    "underground_belt_entrance",
    "underground_belt_exit",
    "splitter",
    "fast_transport_belt",
    "express_transport_belt",
    "fast_underground_belt_entrance",
    "fast_underground_belt_exit",
    "express_underground_belt_entrance",
    "express_underground_belt_exit",
    "fast_splitter",
    "express_splitter",
    "fast_inserter",
    "long_handed_inserter",
    "small_electric_pole",
    "steam_engine",
    "boiler",
    "offshore_pump",
    "pipe",
    "storage_tank",
    "crude_oil_patch",
    "pumpjack",
    "oil_refinery",
    "chemical_plant",
];

const TILE_NAMES: [&str; 8] = [
    "grass", "dirt", "water", "sand", "forest", "snow", "swamp", "rock",
];
const TECHNOLOGY_NAMES: [&str; 10] = [
    "logistics",
    "automation",
    "electric_power",
    "logistic_science_pack",
    "logistics_2",
    "fluid_handling",
    "logistics_3",
    "oil_processing",
    "plastics",
    "sulfur_processing",
];

#[test]
fn base_catalog_loads_from_ron() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    assert_eq!(catalog.items.len(), 54);
    assert_eq!(catalog.fluids.len(), 7);
    assert_eq!(catalog.recipes.len(), 54);
    assert_eq!(catalog.entities.len(), 42);
    assert_eq!(catalog.tiles.len(), 8);
    assert_eq!(catalog.technologies.len(), 22);
}

#[test]
fn base_catalog_contains_expected_names() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for name in ITEM_NAMES {
        assert!(
            catalog.items.iter().any(|prototype| prototype.name == name),
            "missing item {name}"
        );
    }

    for name in FLUID_NAMES {
        assert!(
            catalog
                .fluids
                .iter()
                .any(|prototype| prototype.name == name),
            "missing fluid {name}"
        );
    }

    for name in RECIPE_NAMES {
        assert!(
            catalog
                .recipes
                .iter()
                .any(|prototype| prototype.name == name),
            "missing recipe {name}"
        );
    }

    for name in ENTITY_NAMES {
        assert!(
            catalog
                .entities
                .iter()
                .any(|prototype| prototype.name == name),
            "missing entity {name}"
        );
    }

    for name in TILE_NAMES {
        assert!(
            catalog.tiles.iter().any(|prototype| prototype.name == name),
            "missing tile {name}"
        );
    }

    for name in TECHNOLOGY_NAMES {
        assert!(
            catalog
                .technologies
                .iter()
                .any(|prototype| prototype.name == name),
            "missing technology {name}"
        );
    }
}

#[test]
fn explicit_ids_are_sorted_and_stable() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (expected, item) in catalog.items.iter().enumerate() {
        assert_eq!(item.id.index(), expected);
    }

    for (expected, fluid) in catalog.fluids.iter().enumerate() {
        assert_eq!(fluid.id.index(), expected);
    }

    for (expected, recipe) in catalog.recipes.iter().enumerate() {
        assert_eq!(recipe.id.index(), expected);
    }

    for (expected, entity) in catalog.entities.iter().enumerate() {
        assert_eq!(entity.id.index(), expected);
    }

    for (expected, tile) in catalog.tiles.iter().enumerate() {
        assert_eq!(tile.id.index(), expected);
    }

    for (expected, technology) in catalog.technologies.iter().enumerate() {
        assert_eq!(technology.id.index(), expected);
    }
}
