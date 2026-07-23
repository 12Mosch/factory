use std::collections::BTreeSet;

use crate::catalog::PrototypeCatalog;
use crate::model::{ItemAmount, TechnologyEffect};

use super::common::researchable_technology_ids;

#[test]
fn automation_technology_loads_research_cost_and_unlock_effect() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let logistics = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "logistics")
        .expect("base catalog should contain logistics")
        .id;
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

    assert_eq!(automation.prerequisites, vec![logistics]);
    assert_eq!(
        automation.science_packs,
        vec![ItemAmount {
            item: automation_science_pack,
            amount: 1,
        }]
    );
    assert_eq!(automation.required_units, 20);
    assert_eq!(automation.research_time_ticks, 600);
    assert_eq!(
        automation.effects,
        vec![TechnologyEffect::UnlockRecipe(assembling_machine_recipe)]
    );
}

#[test]
fn military_progression_is_reachable_and_uses_military_science() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let technology = |name: &str| {
        catalog
            .technologies
            .iter()
            .find(|technology| technology.name == name)
            .unwrap()
    };
    let item = |name: &str| {
        catalog
            .items
            .iter()
            .find(|item| item.name == name)
            .unwrap()
            .id
    };
    let advanced_ammunition = technology("advanced_ammunition");
    assert_eq!(
        advanced_ammunition.prerequisites,
        vec![
            technology("turrets").id,
            technology("advanced_material_processing").id,
        ]
    );
    assert_eq!(
        technology("military_science_pack").prerequisites,
        vec![advanced_ammunition.id]
    );
    for name in ["laser_turret", "modular_armor"] {
        let military_technology = technology(name);
        assert_eq!(military_technology.required_units, 100);
        assert!(
            military_technology
                .science_packs
                .iter()
                .any(|pack| pack.item == item("military_science_pack"))
        );
    }
    assert_eq!(
        researchable_technology_ids(&catalog).len(),
        catalog.technologies.len()
    );
}

#[test]
fn green_science_technologies_load_prerequisites_costs_and_unlocks() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let automation = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "automation")
        .expect("base catalog should contain automation")
        .id;
    let logistics = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "logistics")
        .expect("base catalog should contain logistics")
        .id;
    let electric_power = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "electric_power")
        .expect("base catalog should contain electric power")
        .id;
    let logistic_science_pack = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "logistic_science_pack")
        .expect("base catalog should contain logistic science pack technology")
        .id;
    let logistics_2 = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "logistics_2")
        .expect("base catalog should contain logistics 2 technology");
    let logistics_3 = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "logistics_3")
        .expect("base catalog should contain logistics 3 technology");
    let red = catalog
        .items
        .iter()
        .find(|item| item.name == "automation_science_pack")
        .expect("base catalog should contain automation science pack")
        .id;
    let green = catalog
        .items
        .iter()
        .find(|item| item.name == "logistic_science_pack")
        .expect("base catalog should contain logistic science pack")
        .id;
    let fast_transport_belt = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fast_transport_belt")
        .expect("base catalog should contain fast transport belt recipe")
        .id;
    let fast_underground_belt = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fast_underground_belt")
        .expect("base catalog should contain fast underground belt recipe")
        .id;
    let fast_splitter = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fast_splitter")
        .expect("base catalog should contain fast splitter recipe")
        .id;
    let long_handed_inserter = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "long_handed_inserter")
        .expect("base catalog should contain long handed inserter recipe")
        .id;
    let fast_inserter = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fast_inserter")
        .expect("base catalog should contain fast inserter recipe")
        .id;
    let storage_tank = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "storage_tank")
        .expect("base catalog should contain storage tank recipe")
        .id;
    let pipe_to_ground = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "pipe_to_ground")
        .expect("base catalog should contain pipe to ground recipe")
        .id;
    let pump = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "pump")
        .expect("base catalog should contain pump recipe")
        .id;
    let express_transport_belt = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "express_transport_belt")
        .expect("base catalog should contain express transport belt recipe")
        .id;
    let express_underground_belt = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "express_underground_belt")
        .expect("base catalog should contain express underground belt recipe")
        .id;
    let express_splitter = catalog
        .recipes
        .iter()
        .find(|recipe| recipe.name == "express_splitter")
        .expect("base catalog should contain express splitter recipe")
        .id;

    assert_eq!(logistics_2.prerequisites, vec![logistic_science_pack]);
    assert_eq!(
        logistics_2.science_packs,
        vec![
            ItemAmount {
                item: red,
                amount: 1,
            },
            ItemAmount {
                item: green,
                amount: 1,
            },
        ]
    );
    assert_eq!(logistics_2.required_units, 75);
    assert_eq!(
        logistics_2.effects,
        vec![
            TechnologyEffect::UnlockRecipe(fast_transport_belt),
            TechnologyEffect::UnlockRecipe(fast_underground_belt),
            TechnologyEffect::UnlockRecipe(fast_splitter),
            TechnologyEffect::UnlockRecipe(long_handed_inserter),
            TechnologyEffect::UnlockRecipe(fast_inserter),
        ]
    );
    let fluid_handling = catalog
        .technologies
        .iter()
        .find(|technology| technology.name == "fluid_handling")
        .expect("base catalog should contain fluid handling technology");
    assert_eq!(fluid_handling.prerequisites, vec![logistics_2.id]);
    assert_eq!(
        fluid_handling.science_packs,
        vec![
            ItemAmount {
                item: red,
                amount: 1,
            },
            ItemAmount {
                item: green,
                amount: 1,
            },
        ]
    );
    assert_eq!(fluid_handling.required_units, 75);
    assert_eq!(fluid_handling.research_time_ticks, 600);
    assert_eq!(
        fluid_handling.effects,
        vec![
            TechnologyEffect::UnlockRecipe(storage_tank),
            TechnologyEffect::UnlockRecipe(pipe_to_ground),
            TechnologyEffect::UnlockRecipe(pump),
        ]
    );
    assert_eq!(logistics_3.prerequisites, vec![fluid_handling.id]);
    assert_eq!(
        logistics_3.science_packs,
        vec![
            ItemAmount {
                item: red,
                amount: 1,
            },
            ItemAmount {
                item: green,
                amount: 1,
            },
        ]
    );
    assert_eq!(logistics_3.required_units, 150);
    assert_eq!(logistics_3.research_time_ticks, 600);
    assert_eq!(
        logistics_3.effects,
        vec![
            TechnologyEffect::UnlockRecipe(express_transport_belt),
            TechnologyEffect::UnlockRecipe(express_underground_belt),
            TechnologyEffect::UnlockRecipe(express_splitter),
        ]
    );

    let logistic_science_pack_technology = catalog
        .technologies
        .iter()
        .find(|technology| technology.id == logistic_science_pack)
        .expect("technology id should resolve");
    assert_eq!(
        logistic_science_pack_technology.prerequisites,
        vec![electric_power]
    );

    let electric_power_technology = catalog
        .technologies
        .iter()
        .find(|technology| technology.id == electric_power)
        .expect("technology id should resolve");
    assert_eq!(
        electric_power_technology.prerequisites,
        vec![automation, logistics]
    );
}

#[test]
fn early_progression_spine_is_linear_through_fluid_handling() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let expected = [
        "logistics",
        "automation",
        "electric_power",
        "logistic_science_pack",
        "logistics_2",
        "fluid_handling",
    ];
    // Defense and the chemical-science era hang off the spine as optional
    // side branches; researching only spine technologies must still surface
    // each spine step in order.
    let defense_branch = [
        "stone_walls",
        "turrets",
        "electric_mining",
        "electric_energy_distribution_1",
        "advanced_material_processing",
        "engine",
    ];
    let mut completed = BTreeSet::new();

    for technology_name in expected {
        let selectable = catalog
            .technologies
            .iter()
            .filter(|technology| {
                !completed.contains(&technology.id)
                    && !defense_branch.contains(&technology.name.as_str())
                    && technology
                        .prerequisites
                        .iter()
                        .all(|prerequisite| completed.contains(prerequisite))
            })
            .map(|technology| technology.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(selectable, vec![technology_name]);
        completed.insert(crate::technology_id_by_name(&catalog, technology_name));
    }
}

#[test]
fn technology_science_pack_recipes_are_unlocked_before_they_are_required() {
    let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
    let mut unlocked_recipes = catalog
        .recipes
        .iter()
        .filter(|recipe| {
            !catalog.technologies.iter().any(|technology| {
                technology.effects.iter().any(|effect| {
                    matches!(effect, TechnologyEffect::UnlockRecipe(recipe_id) if *recipe_id == recipe.id)
                })
            })
        })
        .map(|recipe| recipe.id)
        .collect::<BTreeSet<_>>();

    for technology_name in [
        "logistics",
        "automation",
        "electric_power",
        "logistic_science_pack",
        "logistics_2",
        "fluid_handling",
        "logistics_3",
    ] {
        let technology = catalog
            .technologies
            .iter()
            .find(|technology| technology.name == technology_name)
            .expect("expected early technology should exist");

        for science_pack in &technology.science_packs {
            let pack_recipe = catalog.recipes.iter().find(|recipe| {
                recipe
                    .products
                    .iter()
                    .any(|product| product.item == science_pack.item)
            });
            if let Some(pack_recipe) = pack_recipe {
                assert!(
                    unlocked_recipes.contains(&pack_recipe.id),
                    "{} requires a science pack whose recipe is not unlocked yet: {}",
                    technology.name,
                    pack_recipe.name
                );
            }
        }

        for effect in &technology.effects {
            let TechnologyEffect::UnlockRecipe(recipe_id) = *effect;
            unlocked_recipes.insert(recipe_id);
        }
    }
}
