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
    input_slot: ItemSlot,
) -> Option<(
    RecipeId,
    u32,
    factory_data::ItemAmount,
    factory_data::ItemAmount,
)> {
    let input_stack = input_slot.stack()?;
    let recipe = first_matching_unlocked_smelting_recipe(catalog, research, input_stack.item_id())?;
    let ingredient = recipe.ingredients[0].clone();
    if input_stack.count() < ingredient.amount {
        return None;
    }
    let product = recipe.products[0].clone();

    Some((recipe.id, recipe.crafting_time_ticks, ingredient, product))
}

pub(in crate::simulation) fn furnace_input_accepts_item(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    item_id: ItemId,
) -> bool {
    first_matching_unlocked_smelting_recipe(catalog, research, item_id).is_some()
}

/// Smelting duration on a specific furnace: the recipe time scaled by the
/// furnace prototype's crafting speed fraction.
pub(in crate::simulation) fn furnace_required_ticks(
    prototype: &factory_data::EntityPrototype,
    recipe_ticks: u32,
) -> u32 {
    let Some(furnace) = prototype.furnace.as_ref() else {
        return recipe_ticks;
    };
    assembler_required_ticks(
        recipe_ticks,
        furnace.crafting_speed_numerator,
        furnace.crafting_speed_denominator,
    )
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
        && state
            .input_inventory
            .slots()
            .iter()
            .all(|slot| slot.is_empty())
        && state
            .output_inventory
            .slots()
            .iter()
            .all(|slot| slot.is_empty())
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

/// Recipe category crafted by the assembling machine `entity_id`. Falls back
/// to `Crafting` when the machine metadata is missing.
pub(in crate::simulation) fn assembler_machine_category(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    entity_id: EntityId,
) -> CraftingCategory {
    entities
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
        .and_then(|prototype| prototype.assembling_machine.as_ref())
        .map(|assembling_machine| assembling_machine.crafting_category)
        .unwrap_or(CraftingCategory::Crafting)
}

pub(in crate::simulation) fn assembler_input_accepts_item(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    machine_category: CraftingCategory,
    state: &AssemblingMachineState,
    item_id: ItemId,
) -> bool {
    let Some(recipe_id) = state.selected_recipe else {
        return false;
    };
    let Some(recipe) = catalog
        .recipe(recipe_id)
        .filter(|recipe| recipe.category == machine_category)
    else {
        return false;
    };
    if !recipe_is_unlocked(catalog, research, recipe.id) {
        return false;
    }

    recipe
        .ingredients
        .iter()
        .any(|ingredient| ingredient.item == item_id)
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
        .slots()
        .iter()
        .filter(|slot| slot.is_empty())
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
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .filter(|stack| stack.item_id() == product.item)
            .map(|stack| stack_size.saturating_sub(u32::from(stack.count())))
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

/// Assigns each fluid ingredient a distinct `Input` fluid box currently
/// holding at least the required amount of that fluid. Returns the box index
/// per ingredient, or `None` when a recipe cannot be satisfied.
pub(in crate::simulation) fn fluid_ingredient_box_indices(
    prototype_boxes: &[factory_data::FluidBoxPrototype],
    box_states: &[FluidBoxState],
    fluid_ingredients: &[factory_data::FluidAmount],
) -> Option<Vec<usize>> {
    let mut used = vec![false; prototype_boxes.len()];
    fluid_ingredients
        .iter()
        .map(|ingredient| {
            let box_index =
                prototype_boxes
                    .iter()
                    .enumerate()
                    .position(|(box_index, prototype_box)| {
                        !used[box_index]
                            && prototype_box.io == factory_data::FluidBoxIo::Input
                            && box_states.get(box_index).is_some_and(|state| {
                                state.fluid_id == Some(ingredient.fluid)
                                    && state.amount_milliunits >= ingredient.amount_milliunits
                            })
                    })?;
            used[box_index] = true;
            Some(box_index)
        })
        .collect()
}

/// Assigns each fluid product a distinct `Output` fluid box that accepts the
/// fluid and has room for the produced amount. Returns the box index per
/// product, or `None` when the outputs cannot be stored.
pub(in crate::simulation) fn fluid_product_box_indices(
    prototype_boxes: &[factory_data::FluidBoxPrototype],
    box_states: &[FluidBoxState],
    fluid_products: &[factory_data::FluidAmount],
) -> Option<Vec<usize>> {
    let mut used = vec![false; prototype_boxes.len()];
    fluid_products
        .iter()
        .map(|product| {
            let box_index =
                prototype_boxes
                    .iter()
                    .enumerate()
                    .position(|(box_index, prototype_box)| {
                        !used[box_index]
                            && prototype_box.io == factory_data::FluidBoxIo::Output
                            && prototype_box
                                .filter
                                .is_none_or(|filter| filter == product.fluid)
                            && box_states.get(box_index).is_some_and(|state| {
                                state.fluid_id.is_none_or(|fluid| fluid == product.fluid)
                                    && prototype_box
                                        .capacity_milliunits
                                        .saturating_sub(state.amount_milliunits)
                                        >= product.amount_milliunits
                            })
                    })?;
            used[box_index] = true;
            Some(box_index)
        })
        .collect()
}

pub(in crate::simulation) fn consume_fluid_ingredients(
    box_states: &mut [FluidBoxState],
    box_indices: &[usize],
    fluid_ingredients: &[factory_data::FluidAmount],
) {
    for (ingredient, &box_index) in fluid_ingredients.iter().zip(box_indices) {
        let state = &mut box_states[box_index];
        debug_assert_eq!(state.fluid_id, Some(ingredient.fluid));
        state.amount_milliunits = state
            .amount_milliunits
            .checked_sub(ingredient.amount_milliunits)
            .expect("fluid ingredient availability was checked before completion");
        if state.amount_milliunits == 0 {
            state.fluid_id = None;
        }
    }
}

pub(in crate::simulation) fn insert_fluid_products(
    box_states: &mut [FluidBoxState],
    box_indices: &[usize],
    fluid_products: &[factory_data::FluidAmount],
) {
    for (product, &box_index) in fluid_products.iter().zip(box_indices) {
        let state = &mut box_states[box_index];
        state.fluid_id = Some(product.fluid);
        state.amount_milliunits += product.amount_milliunits;
    }
}

fn assembler_recipe(
    catalog: &PrototypeCatalog,
    recipe_id: RecipeId,
    machine_category: CraftingCategory,
) -> Result<&factory_data::RecipePrototype, AssemblerError> {
    let recipe = catalog
        .recipe(recipe_id)
        .ok_or(AssemblerError::MissingRecipe(recipe_id))?;
    if recipe.category != machine_category {
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
        let machine_category =
            assembler_machine_category(&self.world.prototypes, &self.entities, entity_id);
        let recipe = assembler_recipe(&self.world.prototypes, recipe_id, machine_category)?;
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
        self.invalidate_consumer_power_demand(entity_id);

        Ok(())
    }

    pub fn can_select_assembler_recipe(
        &self,
        entity_id: EntityId,
        recipe_id: RecipeId,
    ) -> Result<bool, AssemblerError> {
        let machine_category =
            assembler_machine_category(&self.world.prototypes, &self.entities, entity_id);
        assembler_recipe(&self.world.prototypes, recipe_id, machine_category)?;
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
        let machine_category =
            assembler_machine_category(&self.world.prototypes, &self.entities, entity_id);
        let state = self.entities.assembler_state(entity_id)?;
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, &self.research, state)
        else {
            return if let Some(recipe_id) = state.selected_recipe {
                Err(AssemblerError::MissingRecipe(recipe_id))
            } else {
                Ok(Vec::new())
            };
        };
        if recipe.category != machine_category {
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
    fn lab_science_precheck_handles_five_pack_types_and_new_pack_shortages() {
        let catalog = PrototypeCatalog::load_base().expect("base prototype catalog should load");
        let pack_ids = [
            "automation_science_pack",
            "logistic_science_pack",
            "chemical_science_pack",
            "production_science_pack",
            "utility_science_pack",
        ]
        .map(|name| item_id(&catalog, name));
        let science_packs = pack_ids
            .map(|item| factory_data::ItemAmount { item, amount: 1 })
            .to_vec();
        let inventory_with = |included_packs: &[ItemId]| {
            Inventory::from_slots(
                &catalog,
                included_packs
                    .iter()
                    .map(|item| {
                        ItemSlot::from_stack(
                            &catalog,
                            ItemStack::new(&catalog, *item, 1)
                                .expect("science stack should be valid"),
                        )
                        .expect("science slot should be valid")
                    })
                    .collect(),
            )
            .expect("lab test inventory should be valid")
        };

        assert!(lab_has_science_packs(
            &inventory_with(&pack_ids),
            &science_packs
        ));
        assert!(!lab_has_science_packs(
            &inventory_with(&pack_ids[..4]),
            &science_packs
        ));
    }

    #[test]
    fn assembler_inventory_checks_aggregate_duplicate_items_without_temporary_maps() {
        let sim = Simulation::new_test_world(123);
        let iron = item_id(&sim.world.prototypes, "iron_plate");
        let copper = item_id(&sim.world.prototypes, "copper_plate");
        let iron_stack_size =
            item_stack_size(&sim.world.prototypes, iron).expect("iron should have stack size");

        let input_inventory = test_inventory(vec![Some(test_stack(iron, 3))]);
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

        let output_inventory =
            test_inventory(vec![Some(test_stack(iron, iron_stack_size - 1)), None]);
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
            &test_inventory(vec![None]),
            &duplicate_products
        ));
    }
}
