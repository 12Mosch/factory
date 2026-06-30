use super::super::*;

pub(super) fn item_exists(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog
        .items
        .get(item_id.index())
        .is_some_and(|item| item.id == item_id)
}

pub(super) fn fluid_exists(catalog: &PrototypeCatalog, fluid_id: FluidId) -> bool {
    catalog
        .fluids
        .get(fluid_id.index())
        .is_some_and(|fluid| fluid.id == fluid_id)
}

pub(super) fn recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Option<&factory_data::RecipePrototype> {
    catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id)
}

pub(super) fn smelting_recipe_by_id(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Option<&factory_data::RecipePrototype> {
    recipe_by_id(catalog, recipe_id).filter(|recipe| recipe.category == CraftingCategory::Smelting)
}

pub(super) fn technology_by_id(
    catalog: &PrototypeCatalog,
    technology_id: TechnologyId,
) -> Option<&factory_data::TechnologyPrototype> {
    catalog
        .technologies
        .get(technology_id.index())
        .filter(|technology| technology.id == technology_id)
}

pub(super) fn technology_researched(research: &ResearchState, technology_id: TechnologyId) -> bool {
    research
        .technologies
        .get(technology_id.index())
        .filter(|state| state.technology_id == technology_id)
        .is_some_and(|state| state.unlocked)
}

pub(super) fn entity_prototype_by_id(
    catalog: &PrototypeCatalog,
    prototype_id: EntityPrototypeId,
) -> Option<&factory_data::EntityPrototype> {
    catalog
        .entities
        .get(prototype_id.index())
        .filter(|prototype| prototype.id == prototype_id)
}
