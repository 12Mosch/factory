use std::collections::HashMap;

use crate::error::PrototypeLoadError;
use crate::ids::{FluidId, ItemId, RecipeId};
use crate::model::RecipePrototype;
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
            Ok(RecipePrototype {
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
            })
        })
        .collect::<Result<Vec<_>, PrototypeLoadError>>()?;
    let recipe_ids_by_name = recipes
        .iter()
        .map(|recipe: &RecipePrototype| (recipe.name.clone(), recipe.id))
        .collect();

    Ok((recipes, recipe_ids_by_name))
}
