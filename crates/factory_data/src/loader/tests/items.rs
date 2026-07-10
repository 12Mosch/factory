use std::collections::BTreeSet;

use crate::catalog::PrototypeCatalog;
use crate::model::{EntityKind, TechnologyEffect};

use super::common::{recipe_by_id, researchable_technology_ids};

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
fn placeable_items_have_acquisition_paths() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let item_ids = catalog
        .items
        .iter()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let recipe_products = catalog
        .recipes
        .iter()
        .flat_map(|recipe| recipe.products.iter().map(|product| product.item))
        .collect::<BTreeSet<_>>();
    let research_unlocked_recipes = catalog
        .technologies
        .iter()
        .flat_map(|technology| technology.effects.iter())
        .map(|effect| match effect {
            TechnologyEffect::UnlockRecipe(recipe_id) => *recipe_id,
        })
        .collect::<BTreeSet<_>>();
    let starting_inventory_items = ["burner_mining_drill", "stone_furnace"]
        .into_iter()
        .map(|name| crate::item_id_by_name(&catalog, name))
        .collect::<BTreeSet<_>>();
    let mineable_resource_items = ["iron_ore", "copper_ore", "coal", "stone"]
        .into_iter()
        .map(|name| crate::item_id_by_name(&catalog, name))
        .collect::<BTreeSet<_>>();

    for entity in catalog
        .entities
        .iter()
        .filter(|entity| entity.entity_kind != EntityKind::ResourcePatch)
    {
        let Some(build_item) = entity.build_item else {
            continue;
        };

        assert!(
            item_ids.contains(&build_item),
            "{} build item should exist",
            entity.name
        );
        assert!(
            recipe_products.contains(&build_item)
                || starting_inventory_items.contains(&build_item)
                || mineable_resource_items.contains(&build_item),
            "{} build item should have an acquisition path",
            entity.name
        );
    }

    let researchable_technologies = researchable_technology_ids(&catalog);
    assert_eq!(
        researchable_technologies.len(),
        catalog.technologies.len(),
        "every technology should be reachable from prerequisite roots"
    );

    for technology in &catalog.technologies {
        assert!(
            researchable_technologies.contains(&technology.id),
            "{} should be reachable through research",
            technology.name
        );

        for effect in &technology.effects {
            let TechnologyEffect::UnlockRecipe(recipe_id) = *effect;
            let recipe = recipe_by_id(&catalog, recipe_id);

            assert!(
                research_unlocked_recipes.contains(&recipe_id),
                "{} should be unlocked by a technology",
                recipe.name
            );
            assert!(
                !recipe.products.is_empty() || !recipe.fluid_products.is_empty(),
                "{} should produce at least one item or fluid",
                recipe.name
            );
            assert!(
                recipe
                    .products
                    .iter()
                    .all(|product| recipe_products.contains(&product.item)),
                "{} locked products should resolve to catalog recipe products",
                recipe.name
            );
        }
    }
}
