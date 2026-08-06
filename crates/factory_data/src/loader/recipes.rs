use std::collections::HashMap;

use crate::error::PrototypeLoadError;
use crate::ids::{FluidId, ItemId, RecipeId};
use crate::model::{CraftingCategory, RecipePrototype};
use crate::raw::RawRecipePrototype;
use crate::validation::{resolve_fluid_amounts, resolve_item_amounts};

pub(super) fn load_recipes(
    recipes: Vec<RawRecipePrototype>,
    item_ids_by_name: &HashMap<String, ItemId>,
    fluid_ids_by_name: &HashMap<String, FluidId>,
) -> Result<(Vec<RecipePrototype>, HashMap<String, RecipeId>), PrototypeLoadError> {
    let recipes = recipes
        .into_iter()
        .map(|recipe| {
            let recipe_name = recipe.name.clone();
            let recipe = RecipePrototype {
                id: RecipeId::new(recipe.id),
                name: recipe.name,
                category: recipe.category,
                crafting_time_ticks: recipe.crafting_time_ticks,
                ingredients: resolve_item_amounts(
                    &recipe_name,
                    recipe.ingredients,
                    item_ids_by_name,
                )?,
                products: resolve_item_amounts(&recipe_name, recipe.products, item_ids_by_name)?,
                fluid_ingredients: resolve_fluid_amounts(
                    &recipe_name,
                    recipe.fluid_ingredients,
                    fluid_ids_by_name,
                )?,
                fluid_products: resolve_fluid_amounts(
                    &recipe_name,
                    recipe.fluid_products,
                    fluid_ids_by_name,
                )?,
            };
            validate_rocket_building_recipe(&recipe)?;
            Ok(recipe)
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()?;
    validate_single_rocket_building_recipe(&recipes)?;
    let recipe_ids_by_name = recipes
        .iter()
        .map(|recipe: &RecipePrototype| (recipe.name.clone(), recipe.id))
        .collect();

    Ok((recipes, recipe_ids_by_name))
}

/// The rocket-building category holds at most one recipe.
///
/// A silo has nowhere to record which recipe it is building — that is the whole
/// point of the fixed recipe — so it resolves the first unlocked one in the
/// category on every tick. With two, researching the earlier one would switch
/// every silo in the world mid-build: progress earned against one recipe would
/// finish the other, and the ingredients already inserted for the old one would
/// be stranded in slots that no longer accept them. Rejecting the second recipe
/// is what makes "the silo's recipe" a well-defined phrase.
fn validate_single_rocket_building_recipe(
    recipes: &[RecipePrototype],
) -> Result<(), PrototypeLoadError> {
    let mut rocket_building = recipes
        .iter()
        .filter(|recipe| recipe.category == CraftingCategory::RocketBuilding);
    rocket_building.next();
    match rocket_building.next() {
        None => Ok(()),
        Some(second) => Err(PrototypeLoadError::InvalidRocketBuildingRecipe {
            recipe: second.name.clone(),
            detail: "a silo builds the one recipe in its category, so there cannot be a second",
        }),
    }
}

/// Constrains what a rocket silo can be asked to build.
///
/// A silo is not a general crafting machine and its tick loop does not pretend
/// to be one: it has no fluid boxes, and its product is a counter rather than an
/// inventory. Two recipe shapes the ordinary model allows would therefore be
/// silently mishandled — fluid amounts would never be drawn or emitted, making
/// the craft partly free, and a product amount other than one would leave the
/// part counter disagreeing with the production statistics recorded beside it.
///
/// Rejecting both here is what lets the simulation's rocket-part sink count
/// whole crafts. The expressiveness costs nothing: a silo that should build
/// faster says so through `crafting_time_ticks` or its own crafting speed, which
/// is where the rate of a machine belongs, and how many parts make a rocket is
/// the silo prototype's `parts_per_rocket` rather than a second figure here that
/// could contradict it.
fn validate_rocket_building_recipe(recipe: &RecipePrototype) -> Result<(), PrototypeLoadError> {
    if recipe.category != CraftingCategory::RocketBuilding {
        return Ok(());
    }

    let invalid = |detail| {
        Err(PrototypeLoadError::InvalidRocketBuildingRecipe {
            recipe: recipe.name.clone(),
            detail,
        })
    };
    if !recipe.fluid_ingredients.is_empty() || !recipe.fluid_products.is_empty() {
        return invalid("a rocket silo has no fluid boxes, so it can neither draw nor emit fluid");
    }
    match recipe.products.as_slice() {
        [product] if product.amount == 1 => Ok(()),
        _ => invalid("a rocket silo counts whole crafts, so exactly one part must come of each"),
    }
}
