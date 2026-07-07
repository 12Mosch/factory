use std::collections::BTreeSet;

use crate::catalog::PrototypeCatalog;
use crate::ids::{RecipeId, TechnologyId};

pub(super) fn recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> &crate::RecipePrototype {
    catalog.recipe(recipe_id).expect("recipe id should resolve")
}

pub(super) fn researchable_technology_ids(catalog: &PrototypeCatalog) -> BTreeSet<TechnologyId> {
    let mut researchable = BTreeSet::new();
    loop {
        let mut inserted = false;
        for technology in &catalog.technologies {
            if !researchable.contains(&technology.id)
                && technology
                    .prerequisites
                    .iter()
                    .all(|prerequisite| researchable.contains(prerequisite))
            {
                researchable.insert(technology.id);
                inserted = true;
            }
        }

        if !inserted {
            return researchable;
        }
    }
}
