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
    ingredients
        .iter()
        .enumerate()
        .filter(|(index, ingredient)| {
            ingredients[..*index]
                .iter()
                .all(|previous| previous.item != ingredient.item)
        })
        .all(|(_, ingredient)| {
            let required = ingredients
                .iter()
                .filter(|candidate| candidate.item == ingredient.item)
                .map(|candidate| u32::from(candidate.amount))
                .sum();
            input_inventory.count(ingredient.item) >= required
        })
}

pub(in crate::simulation) fn assembler_output_can_accept(
    catalog: &PrototypeCatalog,
    output_inventory: &Inventory,
    products: &[factory_data::ItemAmount],
) -> bool {
    let empty_slots = output_inventory
        .slots
        .iter()
        .filter(|slot| slot.is_none())
        .count();
    let mut needed_empty_slots = 0usize;

    for (index, product) in products.iter().enumerate() {
        if products[..index]
            .iter()
            .any(|previous| previous.item == product.item)
        {
            continue;
        }

        let Some(stack_size) = item_stack_size(catalog, product.item).map(u32::from) else {
            return false;
        };
        let required = products
            .iter()
            .filter(|candidate| candidate.item == product.item)
            .map(|candidate| u32::from(candidate.amount))
            .sum::<u32>();
        let existing_capacity = output_inventory
            .slots
            .iter()
            .filter_map(|slot| slot.as_ref())
            .filter(|stack| stack.item_id == product.item)
            .map(|stack| stack_size.saturating_sub(u32::from(stack.count)))
            .sum::<u32>();
        let remaining = required.saturating_sub(existing_capacity);
        needed_empty_slots =
            needed_empty_slots.saturating_add(remaining.div_ceil(stack_size) as usize);
        if needed_empty_slots > empty_slots {
            return false;
        }
    }

    true
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

fn assembler_recipe(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
) -> Result<&factory_data::RecipePrototype, AssemblerError> {
    let recipe = catalog
        .recipe(recipe_id)
        .ok_or(AssemblerError::MissingRecipe(recipe_id))?;
    if recipe.category != CraftingCategory::Crafting {
        return Err(AssemblerError::InvalidRecipe(recipe_id));
    }
    Ok(recipe)
}

impl Simulation {
    pub fn select_assembler_recipe(
        &mut self,
        entity_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<(), AssemblerError> {
        let recipe = assembler_recipe(&self.world.prototypes, recipe_id)?;
        if !self.is_recipe_unlocked(recipe_id) {
            return Err(AssemblerError::RecipeLocked(recipe_id));
        }

        let state = self.entities.assembler_state_mut(entity_id)?;
        if state.selected_recipe == Some(recipe_id) {
            return Ok(());
        }
        if !assembler_is_empty_for_recipe_change(state) {
            return Err(AssemblerError::RecipeChangeRequiresEmpty { entity_id });
        }

        state.selected_recipe = Some(recipe_id);
        state.crafting_progress_ticks = 0;
        state.crafting_required_ticks = assembler_required_ticks(
            recipe.crafting_time_ticks,
            state.crafting_speed_numerator,
            state.crafting_speed_denominator,
        );

        Ok(())
    }

    pub fn can_select_assembler_recipe(
        &self,
        entity_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<bool, AssemblerError> {
        assembler_recipe(&self.world.prototypes, recipe_id)?;
        if !self.is_recipe_unlocked(recipe_id) {
            return Ok(false);
        }

        let state = self.entities.assembler_state(entity_id)?;
        Ok(state.selected_recipe == Some(recipe_id) || assembler_is_empty_for_recipe_change(state))
    }

    pub fn assembler_ingredient_status(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<AssemblerIngredientStatus>, AssemblerError> {
        let state = self.entities.assembler_state(entity_id)?;
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, &self.research, state)
        else {
            return if let Some(recipe_id) = state.selected_recipe {
                Err(AssemblerError::MissingRecipe(recipe_id))
            } else {
                Ok(Vec::new())
            };
        };
        if recipe.category != CraftingCategory::Crafting {
            return Err(AssemblerError::InvalidRecipe(recipe.id));
        }

        Ok(recipe
            .ingredients
            .iter()
            .map(|ingredient| {
                let required = u32::from(ingredient.amount);
                let available = state.input_inventory.count(ingredient.item);
                AssemblerIngredientStatus {
                    item: ingredient.item,
                    required,
                    available,
                    missing: required.saturating_sub(available),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembler_inventory_checks_aggregate_duplicate_items_without_temporary_maps() {
        let sim = Simulation::new_test_world(123);
        let iron = item_id(&sim.world.prototypes, "iron_plate");
        let copper = item_id(&sim.world.prototypes, "copper_plate");
        let iron_stack_size =
            item_stack_size(&sim.world.prototypes, iron).expect("iron should have stack size");

        let input_inventory = Inventory {
            slots: vec![Some(ItemStack {
                item_id: iron,
                count: 3,
            })],
        };
        let duplicate_ingredients = vec![
            factory_data::ItemAmount {
                item: iron,
                amount: 2,
            },
            factory_data::ItemAmount {
                item: iron,
                amount: 2,
            },
        ];
        assert!(!assembler_has_ingredients(
            &input_inventory,
            &duplicate_ingredients
        ));

        let output_inventory = Inventory {
            slots: vec![
                Some(ItemStack {
                    item_id: iron,
                    count: iron_stack_size - 1,
                }),
                None,
            ],
        };
        let competing_products = vec![
            factory_data::ItemAmount {
                item: iron,
                amount: 2,
            },
            factory_data::ItemAmount {
                item: iron,
                amount: 2,
            },
            factory_data::ItemAmount {
                item: copper,
                amount: 1,
            },
        ];
        assert!(!assembler_output_can_accept(
            &sim.world.prototypes,
            &output_inventory,
            &competing_products
        ));

        let duplicate_products = vec![
            factory_data::ItemAmount {
                item: iron,
                amount: iron_stack_size,
            },
            factory_data::ItemAmount {
                item: iron,
                amount: 1,
            },
        ];
        assert!(!assembler_output_can_accept(
            &sim.world.prototypes,
            &Inventory { slots: vec![None] },
            &duplicate_products
        ));
    }
}
