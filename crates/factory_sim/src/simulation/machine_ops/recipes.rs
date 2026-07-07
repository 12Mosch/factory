use crate::simulation::*;

pub(in crate::simulation) fn lab_has_science_packs(
    inventory: &Inventory,
    science_packs: &[factory_data::ItemAmount],
) -> bool {
    science_packs
        .iter()
        .all(|science_pack| inventory.count(science_pack.item) >= u32::from(science_pack.amount))
}

pub(in crate::simulation) fn recipe_is_unlocked(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    recipe_id: RecipeId,
) -> bool {
    let is_locked_by_technology = catalog.technologies.iter().any(|technology| {
        technology.effects.iter().any(|effect| {
            matches!(effect, TechnologyEffect::UnlockRecipe(unlocked_recipe_id) if *unlocked_recipe_id == recipe_id)
        })
    });
    if !is_locked_by_technology {
        return true;
    }

    catalog.technologies.iter().any(|technology| {
        research
            .technology_state(technology.id)
            .is_some_and(|state| state.unlocked)
            && technology.effects.iter().any(|effect| {
                matches!(effect, TechnologyEffect::UnlockRecipe(unlocked_recipe_id) if *unlocked_recipe_id == recipe_id)
            })
    })
}

pub(in crate::simulation) fn first_matching_unlocked_smelting_recipe<'a>(
    catalog: &'a PrototypeCatalog,
    research: &ResearchState,
    input_item: ItemId,
) -> Option<&'a factory_data::RecipePrototype> {
    catalog.recipes.iter().find(|recipe| {
        recipe.category == CraftingCategory::Smelting
            && recipe.ingredients.len() == 1
            && recipe.products.len() == 1
            && recipe.ingredients[0].item == input_item
            && recipe_is_unlocked(catalog, research, recipe.id)
    })
}

pub(in crate::simulation) fn furnace_work_selection(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    input_slot: Option<ItemStack>,
) -> Option<(
    RecipeId,
    u32,
    factory_data::ItemAmount,
    factory_data::ItemAmount,
)> {
    let input_stack = input_slot?;
    let recipe = first_matching_unlocked_smelting_recipe(catalog, research, input_stack.item_id)?;
    let ingredient = recipe.ingredients[0].clone();
    if input_stack.count < ingredient.amount {
        return None;
    }
    let product = recipe.products[0].clone();

    Some((recipe.id, recipe.crafting_time_ticks, ingredient, product))
}

pub(in crate::simulation) fn input_slot_can_accept(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    input_slot: Option<ItemStack>,
    stack: ItemStack,
) -> bool {
    if first_matching_unlocked_smelting_recipe(catalog, research, stack.item_id).is_none() {
        return false;
    }

    output_slot_can_accept(catalog, input_slot, stack.item_id, stack.count)
}

pub(in crate::simulation) fn assembler_required_ticks(
    recipe_ticks: u32,
    speed_numerator: u32,
    speed_denominator: u32,
) -> u32 {
    let numerator = speed_numerator.max(1);
    let denominator = speed_denominator.max(1);
    recipe_ticks
        .saturating_mul(denominator)
        .saturating_add(numerator - 1)
        / numerator
}

pub(in crate::simulation) fn assembler_is_empty_for_recipe_change(
    state: &AssemblingMachineState,
) -> bool {
    state.crafting_progress_ticks == 0
        && state.input_inventory.slots.iter().all(Option::is_none)
        && state.output_inventory.slots.iter().all(Option::is_none)
}

pub(in crate::simulation) fn selected_assembler_recipe<'a>(
    catalog: &'a PrototypeCatalog,
    research: &ResearchState,
    state: &AssemblingMachineState,
) -> Option<&'a factory_data::RecipePrototype> {
    let recipe_id = state.selected_recipe?;
    catalog
        .recipe(recipe_id)
        .filter(|recipe| recipe_is_unlocked(catalog, research, recipe.id))
}

pub(in crate::simulation) fn assembler_input_can_accept(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    state: &AssemblingMachineState,
    stack: ItemStack,
) -> bool {
    let Some(recipe_id) = state.selected_recipe else {
        return false;
    };
    let Some(recipe) = catalog
        .recipe(recipe_id)
        .filter(|recipe| recipe.category == CraftingCategory::Crafting)
    else {
        return false;
    };
    if !recipe_is_unlocked(catalog, research, recipe.id) {
        return false;
    }

    recipe
        .ingredients
        .iter()
        .any(|ingredient| ingredient.item == stack.item_id)
}

pub(in crate::simulation) fn assembler_has_ingredients(
    input_inventory: &Inventory,
    ingredients: &[factory_data::ItemAmount],
) -> bool {
    let mut required = BTreeMap::<ItemId, u32>::new();
    for ingredient in ingredients {
        *required.entry(ingredient.item).or_default() += u32::from(ingredient.amount);
    }

    required
        .into_iter()
        .all(|(item_id, count)| input_inventory.count(item_id) >= count)
}

pub(in crate::simulation) fn assembler_output_can_accept(
    catalog: &PrototypeCatalog,
    output_inventory: &Inventory,
    products: &[factory_data::ItemAmount],
) -> bool {
    let mut output = output_inventory.clone();
    products
        .iter()
        .all(|product| output.insert(catalog, product.item, product.amount).is_ok())
}

pub(in crate::simulation) fn stack_in_assembler_inventory_slot(
    inventory: &Inventory,
    slot_index: usize,
) -> Result<ItemStack, AssemblerError> {
    inventory
        .slots
        .get(slot_index)
        .ok_or(AssemblerError::InvalidSlot { slot_index })?
        .ok_or(AssemblerError::EmptySlot { slot_index })
}
