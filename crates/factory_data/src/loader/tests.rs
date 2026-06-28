use glam::IVec2;

use crate::catalog::PrototypeCatalog;
use crate::error::PrototypeLoadError;
use crate::ids::TechnologyId;
use crate::model::{
    AssemblingMachinePrototype, CraftingCategory, ElectricEnergySourcePrototype, EntityKind,
    ItemAmount, TechnologyEffect, UndergroundBeltPart,
};

const ITEM_NAMES: [&str; 33] = [
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
];

const RECIPE_NAMES: [&str; 23] = [
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
];

const ENTITY_NAMES: [&str; 28] = [
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
];

const TILE_NAMES: [&str; 3] = ["grass", "dirt", "water"];
const TECHNOLOGY_NAMES: [&str; 1] = ["automation"];

#[test]
fn base_catalog_loads_from_ron() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    assert_eq!(catalog.items.len(), 33);
    assert_eq!(catalog.recipes.len(), 23);
    assert_eq!(catalog.entities.len(), 28);
    assert_eq!(catalog.tiles.len(), 3);
    assert_eq!(catalog.technologies.len(), 1);
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

#[test]
fn recipe_item_references_resolve_to_valid_item_ids() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for recipe in &catalog.recipes {
        for amount in recipe.ingredients.iter().chain(recipe.products.iter()) {
            assert!(amount.item.index() < catalog.items.len());
        }
    }
}

#[test]
fn chest_entity_loads_inventory_slot_count() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let chest = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "chest")
        .expect("base catalog should contain chest entity");

    assert_eq!(chest.inventory_slot_count, Some(16));
}

#[test]
fn lab_entity_loads_inventory_slot_count() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let lab = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "lab")
        .expect("base catalog should contain lab entity");

    assert_eq!(lab.inventory_slot_count, Some(16));
    assert_eq!(
        lab.electric_energy_source,
        Some(ElectricEnergySourcePrototype {
            energy_usage_watts: 60_000,
            drain_watts: 0,
        })
    );
}

#[test]
fn coal_loads_fuel_value() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let coal = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "coal")
        .expect("base catalog should contain coal");
    let iron_ore = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "iron_ore")
        .expect("base catalog should contain iron ore");

    assert_eq!(coal.fuel_value_joules, Some(4_000_000));
    assert_eq!(iron_ore.fuel_value_joules, None);
}

#[test]
fn burner_mining_drill_loads_energy_and_mining_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let drill = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "burner_mining_drill")
        .expect("base catalog should contain burner mining drill");

    assert_eq!(
        drill
            .burner
            .as_ref()
            .map(|burner| burner.energy_usage_watts),
        Some(150_000)
    );
    assert_eq!(
        drill.mining_drill.as_ref().map(|mining| mining.mining_area),
        Some(IVec2::new(2, 2))
    );
    assert_eq!(
        drill
            .mining_drill
            .as_ref()
            .map(|mining| mining.ticks_per_item),
        Some(240)
    );
}

#[test]
fn stone_furnace_loads_burner_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let furnace = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "stone_furnace")
        .expect("base catalog should contain stone furnace");

    assert_eq!(
        furnace
            .burner
            .as_ref()
            .map(|burner| burner.energy_usage_watts),
        Some(90_000)
    );
}

#[test]
fn assembling_machine_loads_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let assembler = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "assembling_machine")
        .expect("base catalog should contain assembling machine");

    assert_eq!(assembler.entity_kind, EntityKind::AssemblingMachine);
    assert_eq!(assembler.size, IVec2::new(3, 3));
    assert_eq!(
        assembler.assembling_machine,
        Some(AssemblingMachinePrototype {
            crafting_speed_numerator: 1,
            crafting_speed_denominator: 2,
            input_slot_count: 4,
            output_slot_count: 1,
        })
    );
    assert_eq!(
        assembler.electric_energy_source,
        Some(ElectricEnergySourcePrototype {
            energy_usage_watts: 75_000,
            drain_watts: 2_500,
        })
    );
}

#[test]
fn inserter_variants_load_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (
        entity_name,
        expected_pickup_offset,
        expected_drop_offset,
        expected_pickup_ticks,
        expected_drop_ticks,
        expected_energy_usage_watts,
        expected_drain_watts,
    ) in [
        (
            "inserter",
            IVec2::new(0, -1),
            IVec2::new(0, 1),
            35,
            35,
            15_100,
            400,
        ),
        (
            "fast_inserter",
            IVec2::new(0, -1),
            IVec2::new(0, 1),
            12,
            12,
            59_300,
            500,
        ),
        (
            "long_handed_inserter",
            IVec2::new(0, -2),
            IVec2::new(0, 2),
            25,
            25,
            21_400,
            400,
        ),
    ] {
        let item = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name} item"));
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name} entity"));
        let inserter = entity
            .inserter
            .as_ref()
            .expect("inserter entity should define inserter metadata");

        assert_eq!(entity.entity_kind, EntityKind::Inserter);
        assert_eq!(entity.build_item, Some(item.id));
        assert_eq!(inserter.pickup_offset, expected_pickup_offset);
        assert_eq!(inserter.drop_offset, expected_drop_offset);
        assert_eq!(inserter.pickup_ticks, expected_pickup_ticks);
        assert_eq!(inserter.drop_ticks, expected_drop_ticks);
        assert_eq!(
            entity.electric_energy_source,
            Some(ElectricEnergySourcePrototype {
                energy_usage_watts: expected_energy_usage_watts,
                drain_watts: expected_drain_watts,
            })
        );
    }
}

#[test]
fn electricity_entities_load_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    let pole = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "small_electric_pole")
        .expect("base catalog should contain small electric pole");
    assert_eq!(pole.entity_kind, EntityKind::ElectricPole);
    assert_eq!(pole.size, IVec2::new(1, 1));
    let pole_metadata = pole
        .electric_pole
        .as_ref()
        .expect("small electric pole should define pole metadata");
    assert_eq!(pole_metadata.supply_area_tiles, IVec2::new(5, 5));
    assert_eq!(pole_metadata.wire_reach_tiles_x2, 15);

    let steam_engine = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "steam_engine")
        .expect("base catalog should contain steam engine");
    assert_eq!(steam_engine.entity_kind, EntityKind::SteamEngine);
    assert_eq!(steam_engine.size, IVec2::new(3, 5));
    assert_eq!(
        steam_engine
            .steam_engine
            .as_ref()
            .map(|metadata| metadata.max_power_output_watts),
        Some(900_000)
    );

    let boiler = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "boiler")
        .expect("base catalog should contain boiler");
    assert_eq!(boiler.entity_kind, EntityKind::Boiler);
    assert_eq!(boiler.size, IVec2::new(2, 3));
    assert_eq!(
        boiler
            .burner
            .as_ref()
            .map(|metadata| metadata.energy_usage_watts),
        Some(1_800_000)
    );
    assert_eq!(
        boiler
            .boiler
            .as_ref()
            .map(|metadata| metadata.steam_output_per_second_milliunits),
        Some(60_000)
    );

    let pump = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "offshore_pump")
        .expect("base catalog should contain offshore pump");
    assert_eq!(pump.entity_kind, EntityKind::OffshorePump);
    assert_eq!(pump.size, IVec2::new(2, 1));
    assert_eq!(
        pump.offshore_pump
            .as_ref()
            .map(|metadata| metadata.pumping_speed_per_second_milliunits),
        Some(1_200_000)
    );
}

#[test]
fn underground_belt_endpoints_load_shared_build_item_and_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let underground_belt = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "underground_belt")
        .expect("base catalog should contain underground belt item")
        .id;
    let entrance = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "underground_belt_entrance")
        .expect("base catalog should contain underground belt entrance");
    let exit = catalog
        .entities
        .iter()
        .find(|prototype| prototype.name == "underground_belt_exit")
        .expect("base catalog should contain underground belt exit");

    assert_eq!(entrance.entity_kind, EntityKind::TransportBelt);
    assert_eq!(exit.entity_kind, EntityKind::TransportBelt);
    assert_eq!(entrance.build_item, Some(underground_belt));
    assert_eq!(exit.build_item, Some(underground_belt));
    assert_eq!(
        entrance
            .transport_belt
            .as_ref()
            .and_then(|belt| belt.underground.as_ref())
            .map(|underground| (underground.part, underground.max_distance)),
        Some((UndergroundBeltPart::Entrance, 4))
    );
    assert_eq!(
        exit.transport_belt
            .as_ref()
            .and_then(|belt| belt.underground.as_ref())
            .map(|underground| (underground.part, underground.max_distance)),
        Some((UndergroundBeltPart::Exit, 4))
    );
}

#[test]
fn transport_belt_tiers_load_speed_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (entity_name, expected_speed) in [
        ("transport_belt", 8),
        ("fast_transport_belt", 16),
        ("express_transport_belt", 24),
    ] {
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));

        assert_eq!(entity.entity_kind, EntityKind::TransportBelt);
        assert_eq!(
            entity
                .transport_belt
                .as_ref()
                .map(|belt| (belt.speed_subtiles_per_tick, belt.underground.as_ref())),
            Some((expected_speed, None)),
            "{entity_name} should define straight belt metadata"
        );
    }
}

#[test]
fn underground_belt_tiers_load_speed_and_distance_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (base_name, item_name, expected_speed, expected_distance) in [
        ("underground_belt", "underground_belt", 8, 4),
        ("fast_underground_belt", "fast_underground_belt", 16, 6),
        (
            "express_underground_belt",
            "express_underground_belt",
            24,
            8,
        ),
    ] {
        let item = catalog
            .items
            .iter()
            .find(|prototype| prototype.name == item_name)
            .unwrap_or_else(|| panic!("base catalog should contain {item_name} item"))
            .id;
        for (suffix, expected_part) in [
            ("entrance", UndergroundBeltPart::Entrance),
            ("exit", UndergroundBeltPart::Exit),
        ] {
            let entity_name = format!("{base_name}_{suffix}");
            let entity = catalog
                .entities
                .iter()
                .find(|prototype| prototype.name == entity_name)
                .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));
            let belt = entity
                .transport_belt
                .as_ref()
                .expect("underground endpoint should define belt metadata");
            let underground = belt
                .underground
                .as_ref()
                .expect("underground endpoint should define underground metadata");

            assert_eq!(entity.entity_kind, EntityKind::TransportBelt);
            assert_eq!(entity.build_item, Some(item));
            assert_eq!(belt.speed_subtiles_per_tick, expected_speed);
            assert_eq!(underground.part, expected_part);
            assert_eq!(underground.max_distance, expected_distance);
        }
    }
}

#[test]
fn splitter_tiers_load_speed_metadata() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    for (entity_name, expected_speed) in [
        ("splitter", 8),
        ("fast_splitter", 16),
        ("express_splitter", 24),
    ] {
        let entity = catalog
            .entities
            .iter()
            .find(|prototype| prototype.name == entity_name)
            .unwrap_or_else(|| panic!("base catalog should contain {entity_name}"));

        assert_eq!(entity.entity_kind, EntityKind::Splitter);
        assert_eq!(
            entity
                .splitter
                .as_ref()
                .map(|splitter| splitter.speed_subtiles_per_tick),
            Some(expected_speed)
        );
    }
}

#[test]
fn stone_brick_smelting_recipe_loads() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let stone = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "stone")
        .expect("base catalog should contain stone")
        .id;
    let stone_brick = catalog
        .items
        .iter()
        .find(|prototype| prototype.name == "stone_brick")
        .expect("base catalog should contain stone brick")
        .id;
    let recipe = catalog
        .recipes
        .iter()
        .find(|prototype| prototype.name == "stone_brick")
        .expect("base catalog should contain stone brick recipe");

    assert_eq!(recipe.category, CraftingCategory::Smelting);
    assert_eq!(recipe.crafting_time_ticks, 210);
    assert_eq!(
        recipe.ingredients,
        vec![ItemAmount {
            item: stone,
            amount: 1
        }]
    );
    assert_eq!(
        recipe.products,
        vec![ItemAmount {
            item: stone_brick,
            amount: 1
        }]
    );
}

#[test]
fn automation_technology_loads_research_cost_and_unlock_effect() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let automation_science_pack = catalog
        .items
        .iter()
        .find(|item| item.name == "automation_science_pack")
        .expect("base catalog should contain automation science pack")
        .id;
    let assembling_machine_recipe = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "assembling_machine")
        .expect("base catalog should contain assembling machine recipe")
        .id;
    let automation = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "automation")
        .expect("base catalog should contain automation technology");

    assert_eq!(automation.prerequisites, Vec::<TechnologyId>::new());
    assert_eq!(
        automation.science_packs,
        vec![ItemAmount {
            item: automation_science_pack,
            amount: 1,
        }]
    );
    assert_eq!(automation.required_units, 10);
    assert_eq!(automation.research_time_ticks, 600);
    assert_eq!(
        automation.effects,
        vec![TechnologyEffect::UnlockRecipe(assembling_machine_recipe)]
    );
}

#[test]
fn duplicate_ids_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [
                (id: 0, name: "iron_ore", stack_size: 100),
                (id: 0, name: "copper_ore", stack_size: 100),
            ],
            recipes: [],
            entities: [],
            tiles: [],
        )
        "#,
    )
    .expect_err("duplicate item ids should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateId {
            group: "items",
            id: 0,
        }
    ));
}

#[test]
fn duplicate_names_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [
                (id: 0, name: "iron_ore", stack_size: 100),
                (id: 1, name: "iron_ore", stack_size: 100),
            ],
            recipes: [],
            entities: [],
            tiles: [],
        )
        "#,
    )
    .expect_err("duplicate item names should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateName {
            group: "items",
            name,
        } if name == "iron_ore"
    ));
}

#[test]
fn missing_item_references_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [(id: 0, name: "iron_plate", stack_size: 100)],
            recipes: [(
                id: 0,
                name: "missing_recipe",
                category: Crafting,
                crafting_time_ticks: 30,
                ingredients: [(item: "missing_item", amount: 1)],
                products: [(item: "iron_plate", amount: 1)],
            )],
            entities: [],
            tiles: [],
        )
        "#,
    )
    .expect_err("missing item references should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingItemReference { recipe, item }
            if recipe == "missing_recipe" && item == "missing_item"
    ));
}

#[test]
fn invalid_collision_layers_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [(
                id: 0,
                name: "bad_entity",
                entity_kind: Furnace,
                size: (x: 2, y: 2),
                collision_mask: (layers: ["invalid"]),
            )],
            tiles: [],
        )
        "#,
    )
    .expect_err("invalid collision layers should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidCollisionLayer { owner, layer }
            if owner == "bad_entity" && layer == "invalid"
    ));
}

#[test]
fn missing_technology_prerequisites_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: ["missing"],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("missing technology prerequisites should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingTechnologyPrerequisite {
            technology,
            prerequisite,
        } if technology == "automation" && prerequisite == "missing"
    ));
}

#[test]
fn missing_technology_science_pack_items_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [(item: "missing_pack", amount: 1)],
                required_units: 1,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("missing science pack item should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingTechnologySciencePackItem {
            technology,
            item,
        } if technology == "automation" && item == "missing_pack"
    ));
}

#[test]
fn missing_technology_unlock_recipes_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [UnlockRecipe("missing_recipe")],
            )],
        )
        "#,
    )
    .expect_err("missing unlock recipe should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::MissingTechnologyUnlockRecipe {
            technology,
            recipe,
        } if technology == "automation" && recipe == "missing_recipe"
    ));
}

#[test]
fn duplicate_technology_ids_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [
                (
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
                (
                    id: 0,
                    name: "logistics",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
            ],
        )
        "#,
    )
    .expect_err("duplicate technology ids should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateId {
            group: "technologies",
            id: 0,
        }
    ));
}

#[test]
fn duplicate_technology_names_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [
                (
                    id: 0,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
                (
                    id: 1,
                    name: "automation",
                    prerequisites: [],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
            ],
        )
        "#,
    )
    .expect_err("duplicate technology names should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::DuplicateName {
            group: "technologies",
            name,
        } if name == "automation"
    ));
}

#[test]
fn invalid_technology_required_units_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [],
                required_units: 0,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("zero required units should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidTechnologyRequiredUnits { technology }
            if technology == "automation"
    ));
}

#[test]
fn invalid_technology_research_time_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: [],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 0,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("zero research time should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::InvalidTechnologyResearchTime { technology }
            if technology == "automation"
    ));
}

#[test]
fn technology_self_prerequisites_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [(
                id: 0,
                name: "automation",
                prerequisites: ["automation"],
                science_packs: [],
                required_units: 1,
                research_time_ticks: 1,
                effects: [],
            )],
        )
        "#,
    )
    .expect_err("self prerequisites should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::TechnologySelfPrerequisite { technology }
            if technology == "automation"
    ));
}

#[test]
fn technology_prerequisite_cycles_fail() {
    let error = PrototypeCatalog::from_ron_str(
        r#"
        (
            items: [],
            recipes: [],
            entities: [],
            tiles: [],
            technologies: [
                (
                    id: 0,
                    name: "automation",
                    prerequisites: ["logistics"],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
                (
                    id: 1,
                    name: "logistics",
                    prerequisites: ["automation"],
                    science_packs: [],
                    required_units: 1,
                    research_time_ticks: 1,
                    effects: [],
                ),
            ],
        )
        "#,
    )
    .expect_err("technology prerequisite cycles should fail");

    assert!(matches!(
        error,
        PrototypeLoadError::TechnologyPrerequisiteCycle { .. }
    ));
}
