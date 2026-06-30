use crate::catalog::PrototypeCatalog;
use crate::model::{CraftingCategory, ItemAmount};

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

fn assert_recipe(
    catalog: &PrototypeCatalog,
    recipe_name: &str,
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
    assert_eq!(recipe.crafting_time_ticks, 30);
    assert_eq!(recipe.ingredients, ingredients);
    assert_eq!(recipe.products, products);
}

fn expected_item_amounts(catalog: &PrototypeCatalog, amounts: &[(&str, u16)]) -> Vec<ItemAmount> {
    amounts
        .iter()
        .map(|(name, amount)| ItemAmount {
            item: crate::item_id_by_name(catalog, name),
            amount: *amount,
        })
        .collect()
}
