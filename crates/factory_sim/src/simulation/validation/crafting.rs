use super::super::*;

pub(super) fn validate_crafting_queue(sim: &Simulation) -> Result<(), SimValidationError> {
    for job in &sim.crafting_queue.entries {
        let Some(recipe) = sim.world.prototypes.recipe(job.recipe_id) else {
            return Err(SimValidationError::InvalidCraftingRecipe {
                recipe_id: job.recipe_id,
            });
        };
        if !matches!(
            recipe.category,
            CraftingCategory::Crafting | CraftingCategory::Manual
        ) {
            return Err(SimValidationError::InvalidCraftingRecipe {
                recipe_id: job.recipe_id,
            });
        }
    }

    Ok(())
}
