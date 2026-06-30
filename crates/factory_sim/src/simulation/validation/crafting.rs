use super::super::*;
use super::ids::*;

pub(super) fn validate_crafting_queue(sim: &Simulation) -> Result<(), SimValidationError> {
    for job in &sim.crafting_queue.entries {
        let Some(recipe) = recipe_by_id(&sim.world.prototypes, job.recipe_id) else {
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
