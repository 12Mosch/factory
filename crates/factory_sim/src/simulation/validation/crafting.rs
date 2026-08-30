use super::super::*;

pub(super) fn validate_crafting_queue(sim: &Simulation) -> Result<(), SimValidationError> {
    let mut job_ids = BTreeSet::new();
    for job in &sim.crafting_queue.entries {
        if !job_ids.insert(job.id) || job.id.0 >= sim.crafting_queue.next_job_id {
            return Err(SimValidationError::InvalidCraftingJobIdentity { job_id: job.id });
        }
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
