use factory_data::{CraftingCategory, FluidId, ItemId, PrototypeCatalog};
use factory_sim::{EntityId, ItemStack, Simulation};

use crate::utils::compact_item_name;

pub(crate) fn format_item_stack(stack: ItemStack, catalog: &PrototypeCatalog) -> String {
    let name = catalog
        .item(stack.item_id)
        .map(|item| item.name.as_str())
        .unwrap_or("unknown");
    format!("{}\n{}", compact_item_name(name), stack.count)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssemblerDetailText {
    pub recipe: String,
    pub ingredients: String,
    pub products: String,
    pub progress: String,
}

impl AssemblerDetailText {
    pub(crate) fn empty() -> Self {
        Self {
            recipe: "Recipe: <none>".to_string(),
            ingredients: "Ingredients: <none>".to_string(),
            products: "Output: <none>".to_string(),
            progress: "Progress: 0/0".to_string(),
        }
    }
}

pub fn crafting_recipe_choices(catalog: &PrototypeCatalog) -> Vec<&factory_data::RecipePrototype> {
    catalog
        .recipes
        .iter()
        .filter(|recipe| recipe.category == CraftingCategory::Crafting)
        .collect()
}

pub fn available_crafting_recipe_choices(sim: &Simulation) -> Vec<&factory_data::RecipePrototype> {
    sim.available_recipes(CraftingCategory::Crafting)
}

pub fn format_assembler_detail_text(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<AssemblerDetailText> {
    let state = sim.assembler_state(entity_id).ok()?;
    let Some(recipe) = state
        .selected_recipe
        .and_then(|recipe_id| sim.catalog().recipe(recipe_id))
    else {
        return Some(AssemblerDetailText::empty());
    };

    let statuses = sim.assembler_ingredient_status(entity_id).ok()?;
    let ingredients = if statuses.is_empty() {
        "Ingredients: <none>".to_string()
    } else {
        format!(
            "Ingredients:\n{}",
            statuses
                .iter()
                .map(|status| {
                    format!(
                        "{}: need {}, have {}, missing {}",
                        format_item_display_name(sim.catalog(), status.item),
                        status.required,
                        status.available,
                        status.missing
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    let products = if recipe.products.is_empty() {
        "Output: <none>".to_string()
    } else {
        format!(
            "Output: {}",
            recipe
                .products
                .iter()
                .map(|product| format!(
                    "{} x{}",
                    format_item_display_name(sim.catalog(), product.item),
                    product.amount
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    Some(AssemblerDetailText {
        recipe: format!("Recipe: {}", format_recipe_display_name(&recipe.name)),
        ingredients,
        products,
        progress: format!(
            "Progress: {}/{}",
            state.crafting_progress_ticks, state.crafting_required_ticks
        ),
    })
}

pub(crate) fn format_item_display_name(catalog: &PrototypeCatalog, item_id: ItemId) -> String {
    catalog
        .item(item_id)
        .map(|item| format_recipe_display_name(&item.name))
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn format_fluid_display_name(catalog: &PrototypeCatalog, fluid_id: FluidId) -> String {
    catalog
        .fluid(fluid_id)
        .map(|fluid| format_recipe_display_name(&fluid.name))
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn format_recipe_display_name(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
