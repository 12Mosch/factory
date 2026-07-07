use super::super::*;

pub(super) fn item_exists(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog.item(item_id).is_some()
}

pub(super) fn fluid_exists(catalog: &PrototypeCatalog, fluid_id: FluidId) -> bool {
    catalog.fluid(fluid_id).is_some()
}

pub(super) fn smelting_recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Option<&factory_data::RecipePrototype> {
    catalog
        .recipe(recipe_id)
        .filter(|recipe| recipe.category == CraftingCategory::Smelting)
}

pub(super) fn technology_researched(research: &ResearchState, technology_id: TechnologyId) -> bool {
    research
        .technology_state(technology_id)
        .is_some_and(|state| state.unlocked)
}
