use crate::catalog::PrototypeCatalog;
use crate::model::{CraftingCategory, ItemAmount};

use super::common::expected_item_amounts;

struct ExpectedCraftingRecipe<'a> {
    name: &'a str,
    crafting_time_ticks: u32,
    ingredients: &'a [(&'a str, u16)],
    products: &'a [(&'a str, u16)],
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
fn logistic_science_pack_item_and_recipe_resolve() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let inserter = catalog
        .items
        .iter()
        .find(|item| item.name == "inserter")
        .expect("base catalog should contain inserter")
        .id;
    let transport_belt = catalog
        .items
        .iter()
        .find(|item| item.name == "transport_belt")
        .expect("base catalog should contain transport belt")
        .id;
    let logistic_science_pack = catalog
        .items
        .iter()
        .find(|item| item.name == "logistic_science_pack")
        .expect("base catalog should contain logistic science pack");
    let recipe = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "logistic_science_pack")
        .expect("base catalog should contain logistic science pack recipe");

    assert_eq!(logistic_science_pack.stack_size, 200);
    assert_eq!(recipe.category, CraftingCategory::Crafting);
    assert_eq!(recipe.crafting_time_ticks, 360);
    assert_eq!(
        recipe.ingredients,
        vec![
            ItemAmount {
                item: inserter,
                amount: 1,
            },
            ItemAmount {
                item: transport_belt,
                amount: 1,
            },
        ]
    );
    assert_eq!(
        recipe.products,
        vec![ItemAmount {
            item: logistic_science_pack.id,
            amount: 1,
        }]
    );
}

#[test]
fn express_logistics_recipes_resolve_expected_items() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");

    assert_recipe(
        &catalog,
        "express_transport_belt",
        &[("fast_transport_belt", 1), ("iron_gear_wheel", 10)],
        &[("express_transport_belt", 1)],
    );
    assert_recipe(
        &catalog,
        "express_underground_belt",
        &[("fast_underground_belt", 2), ("iron_gear_wheel", 80)],
        &[("express_underground_belt", 2)],
    );
    assert_recipe(
        &catalog,
        "express_splitter",
        &[
            ("fast_splitter", 1),
            ("iron_gear_wheel", 10),
            ("electronic_circuit", 10),
        ],
        &[("express_splitter", 1)],
    );
}

#[test]
fn production_and_utility_science_items_and_recipes_load_exactly() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let expected_items = [
        (66, "low_density_structure", 50),
        (67, "processing_unit", 100),
        (68, "flying_robot_frame", 50),
        (69, "production_science_pack", 200),
        (70, "utility_science_pack", 200),
    ];

    for (id, name, stack_size) in expected_items {
        let item = catalog
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap_or_else(|| panic!("base catalog should contain {name}"));
        assert_eq!(item.id.index(), id);
        assert_eq!(item.stack_size, stack_size);
        let recipe = catalog
            .recipes
            .iter()
            .find(|recipe| recipe.name == name)
            .unwrap_or_else(|| panic!("base catalog should contain {name} recipe"));
        assert_eq!(recipe.id.index(), id);
    }

    let expected_recipes = [
        ExpectedCraftingRecipe {
            name: "low_density_structure",
            crafting_time_ticks: 1_200,
            ingredients: &[("copper_plate", 20), ("steel_plate", 2), ("plastic_bar", 5)],
            products: &[("low_density_structure", 1)],
        },
        ExpectedCraftingRecipe {
            name: "processing_unit",
            crafting_time_ticks: 600,
            ingredients: &[
                ("electronic_circuit", 20),
                ("advanced_circuit", 2),
                ("sulfur", 5),
            ],
            products: &[("processing_unit", 1)],
        },
        ExpectedCraftingRecipe {
            name: "flying_robot_frame",
            crafting_time_ticks: 1_200,
            ingredients: &[
                ("engine_unit", 1),
                ("advanced_circuit", 2),
                ("steel_plate", 1),
                ("electronic_circuit", 3),
            ],
            products: &[("flying_robot_frame", 1)],
        },
        ExpectedCraftingRecipe {
            name: "production_science_pack",
            crafting_time_ticks: 1_260,
            ingredients: &[
                ("electric_furnace", 1),
                ("advanced_circuit", 1),
                ("stone_brick", 10),
            ],
            products: &[("production_science_pack", 3)],
        },
        ExpectedCraftingRecipe {
            name: "utility_science_pack",
            crafting_time_ticks: 1_260,
            ingredients: &[
                ("low_density_structure", 3),
                ("processing_unit", 2),
                ("flying_robot_frame", 1),
            ],
            products: &[("utility_science_pack", 3)],
        },
    ];
    for expected in expected_recipes {
        assert_crafting_recipe(
            &catalog,
            expected.name,
            expected.crafting_time_ticks,
            expected.ingredients,
            expected.products,
        );
    }
}

#[test]
fn radar_item_and_recipe_load_exactly() {
    let catalog = PrototypeCatalog::load_base().expect("base catalog should load");
    let radar = catalog
        .items
        .iter()
        .find(|item| item.name == "radar")
        .expect("base catalog should contain radar item");

    assert_eq!(radar.id.index(), 84);
    assert_crafting_recipe(
        &catalog,
        "radar",
        30,
        &[
            ("electronic_circuit", 5),
            ("iron_gear_wheel", 5),
            ("iron_plate", 10),
        ],
        &[("radar", 1)],
    );
}

fn assert_recipe(
    catalog: &PrototypeCatalog,
    recipe_name: &str,
    expected_ingredients: &[(&str, u16)],
    expected_products: &[(&str, u16)],
) {
    assert_crafting_recipe(
        catalog,
        recipe_name,
        30,
        expected_ingredients,
        expected_products,
    );
}

fn assert_crafting_recipe(
    catalog: &PrototypeCatalog,
    recipe_name: &str,
    crafting_time_ticks: u32,
    expected_ingredients: &[(&str, u16)],
    expected_products: &[(&str, u16)],
) {
    let recipe = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == recipe_name)
        .unwrap_or_else(|| panic!("base catalog should contain {recipe_name} recipe"));
    let ingredients = expected_item_amounts(catalog, expected_ingredients);
    let products = expected_item_amounts(catalog, expected_products);

    assert_eq!(recipe.category, CraftingCategory::Crafting);
    assert_eq!(recipe.crafting_time_ticks, crafting_time_ticks);
    assert_eq!(recipe.ingredients, ingredients);
    assert_eq!(recipe.products, products);
}
