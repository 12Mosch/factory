use factory_data::{
    CraftingCategory, ItemAmount, ItemId, PrototypeCatalog, RecipeId, RecipePrototype,
    TechnologyEffect,
};
use factory_sim::{Inventory, Simulation};
use std::collections::BTreeMap;

use crate::resources::CraftingPanelTab;
use crate::ui::formatting::{format_item_display_name, format_recipe_display_name};

use super::components::{CraftingPanelSnapshot, ManualCraftRecipeRow};

pub(crate) fn crafting_panel_snapshot(
    sim: &Simulation,
    selected_tab: CraftingPanelTab,
) -> CraftingPanelSnapshot {
    CraftingPanelSnapshot {
        selected_tab,
        rows: recipe_rows(sim, selected_tab),
    }
}

fn recipe_rows(sim: &Simulation, selected_tab: CraftingPanelTab) -> Vec<ManualCraftRecipeRow> {
    sim.catalog()
        .recipes
        .iter()
        .filter(|recipe| recipe_visible_in_tab(recipe, selected_tab))
        .map(|recipe| recipe_row(sim, selected_tab, recipe))
        .collect()
}

fn recipe_visible_in_tab(recipe: &RecipePrototype, selected_tab: CraftingPanelTab) -> bool {
    match selected_tab {
        CraftingPanelTab::Player => {
            matches!(
                recipe.category,
                CraftingCategory::Manual | CraftingCategory::Crafting
            )
        }
        CraftingPanelTab::Smelting => recipe.category == CraftingCategory::Smelting,
        CraftingPanelTab::Assembling => recipe.category == CraftingCategory::Crafting,
    }
}

fn recipe_row(
    sim: &Simulation,
    selected_tab: CraftingPanelTab,
    recipe: &RecipePrototype,
) -> ManualCraftRecipeRow {
    let ingredients = aggregate_amounts(&recipe.ingredients);
    let ingredient_statuses =
        ingredient_statuses(sim.catalog(), sim.player_inventory(), &ingredients);
    let unlocked = sim.is_recipe_unlocked(recipe.id);
    let button_enabled =
        recipe_startable_for_tab(sim, selected_tab, recipe, &ingredients, unlocked);
    let status = recipe_status(sim, selected_tab, recipe, &ingredients, unlocked);

    ManualCraftRecipeRow {
        recipe_id: recipe.id,
        display_name: format_recipe_display_name(&recipe.name),
        products: product_text(sim.catalog(), &recipe.products),
        ingredients: ingredient_statuses,
        status,
        button_enabled,
    }
}

fn recipe_startable_for_tab(
    sim: &Simulation,
    selected_tab: CraftingPanelTab,
    recipe: &RecipePrototype,
    ingredients: &BTreeMap<ItemId, u32>,
    unlocked: bool,
) -> bool {
    selected_tab == CraftingPanelTab::Player
        && matches!(
            recipe.category,
            CraftingCategory::Manual | CraftingCategory::Crafting
        )
        && unlocked
        && has_ingredients(sim.player_inventory(), ingredients)
}

pub(crate) fn craftable_for_player(sim: &Simulation, recipe_id: RecipeId) -> bool {
    let Some(recipe) = recipe_by_id(sim.catalog(), recipe_id) else {
        return false;
    };
    let ingredients = aggregate_amounts(&recipe.ingredients);
    recipe_startable_for_tab(
        sim,
        CraftingPanelTab::Player,
        recipe,
        &ingredients,
        sim.is_recipe_unlocked(recipe_id),
    )
}

fn recipe_status(
    sim: &Simulation,
    selected_tab: CraftingPanelTab,
    recipe: &RecipePrototype,
    ingredients: &BTreeMap<ItemId, u32>,
    unlocked: bool,
) -> String {
    if !unlocked {
        return format!(
            "Locked: {}",
            locking_technology_name(sim.catalog(), recipe.id)
        );
    }

    match selected_tab {
        CraftingPanelTab::Smelting => "Requires furnace".to_string(),
        CraftingPanelTab::Assembling => "Use assembling machine".to_string(),
        CraftingPanelTab::Player => {
            let missing = missing_ingredients(sim.catalog(), sim.player_inventory(), ingredients);
            if missing.is_empty() {
                "Craft".to_string()
            } else {
                format!("Missing: {}", missing.join(", "))
            }
        }
    }
}

fn aggregate_amounts(amounts: &[ItemAmount]) -> BTreeMap<ItemId, u32> {
    let mut aggregated = BTreeMap::new();
    for amount in amounts {
        *aggregated.entry(amount.item).or_insert(0) += u32::from(amount.amount);
    }
    aggregated
}

fn product_text(catalog: &PrototypeCatalog, products: &[ItemAmount]) -> String {
    if products.is_empty() {
        return "Products: <none>".to_string();
    }

    format!(
        "Products: {}",
        products
            .iter()
            .map(|product| format!(
                "{} x{}",
                format_item_display_name(catalog, product.item),
                product.amount
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn ingredient_statuses(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
    ingredients: &BTreeMap<ItemId, u32>,
) -> String {
    if ingredients.is_empty() {
        return "Ingredients: <none>".to_string();
    }

    format!(
        "Ingredients: {}",
        ingredients
            .iter()
            .map(|(item_id, needed)| {
                format!(
                    "{} {}/{}",
                    format_item_display_name(catalog, *item_id),
                    inventory.count(*item_id),
                    needed
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn has_ingredients(inventory: &Inventory, ingredients: &BTreeMap<ItemId, u32>) -> bool {
    ingredients
        .iter()
        .all(|(item_id, needed)| inventory.count(*item_id) >= *needed)
}

fn missing_ingredients(
    catalog: &PrototypeCatalog,
    inventory: &Inventory,
    ingredients: &BTreeMap<ItemId, u32>,
) -> Vec<String> {
    ingredients
        .iter()
        .filter_map(|(item_id, needed)| {
            let have = inventory.count(*item_id);
            (have < *needed).then(|| {
                format!(
                    "{} x{}",
                    format_item_display_name(catalog, *item_id),
                    needed - have
                )
            })
        })
        .collect()
}

fn locking_technology_name(catalog: &PrototypeCatalog, recipe_id: RecipeId) -> String {
    catalog
        .technologies
        .iter()
        .find(|technology| {
            technology.effects.iter().any(|effect| {
                matches!(effect, TechnologyEffect::UnlockRecipe(unlocked_id) if *unlocked_id == recipe_id)
            })
        })
        .map(|technology| format_recipe_display_name(&technology.name))
        .unwrap_or_else(|| "Technology".to_string())
}

pub(crate) fn queue_snapshot(sim: &Simulation) -> Vec<String> {
    sim.crafting_queue()
        .entries
        .iter()
        .enumerate()
        .map(|(index, job)| {
            let recipe_name = recipe_by_id(sim.catalog(), job.recipe_id)
                .map(|recipe| format_recipe_display_name(&recipe.name))
                .unwrap_or_else(|| "Unknown".to_string());
            let mut line = format!("{recipe_name}: {} ticks remaining", job.remaining_ticks);
            if index == 0 && job.remaining_ticks == 0 && front_job_waiting_for_space(sim) {
                line.push_str(" - Waiting for inventory space");
            }
            line
        })
        .collect()
}

fn front_job_waiting_for_space(sim: &Simulation) -> bool {
    let Some(job) = sim.crafting_queue().entries.front() else {
        return false;
    };
    if job.remaining_ticks != 0 {
        return false;
    }
    let Some(recipe) = recipe_by_id(sim.catalog(), job.recipe_id) else {
        return false;
    };

    let mut inventory = sim.player_inventory().clone();
    for product in &recipe.products {
        if inventory
            .insert(sim.catalog(), product.item, product.amount)
            .is_err()
        {
            return true;
        }
    }
    false
}

fn recipe_by_id(catalog: &PrototypeCatalog, recipe_id: RecipeId) -> Option<&RecipePrototype> {
    catalog
        .recipes
        .get(recipe_id.index())
        .filter(|recipe| recipe.id == recipe_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::{item_id_by_name, recipe_id_by_name};

    #[test]
    fn manual_craft_ui_shows_locked_recipes_as_disabled() {
        let sim = Simulation::new_test_world(123);
        let recipe_id = recipe_id_by_name(sim.catalog(), "assembling_machine");
        let row = row_for_recipe(&sim, CraftingPanelTab::Player, recipe_id);

        assert!(!row.button_enabled);
        assert_eq!(row.status, "Locked: Automation");
    }

    #[test]
    fn manual_craft_ui_preserves_locked_status_on_assembling_tab() {
        let sim = Simulation::new_test_world(123);
        let recipe_id = recipe_id_by_name(sim.catalog(), "assembling_machine");
        let row = row_for_recipe(&sim, CraftingPanelTab::Assembling, recipe_id);

        assert!(!row.button_enabled);
        assert_eq!(row.status, "Locked: Automation");
    }

    #[test]
    fn manual_craft_ui_enables_unlocked_recipe_with_ingredients() {
        let mut sim = Simulation::new_test_world(123);
        let catalog = sim.catalog().clone();
        let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
        sim.player_inventory_mut()
            .insert(&catalog, iron_plate, 2)
            .expect("test inventory should accept iron plates");
        let recipe_id = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");

        let row = row_for_recipe(&sim, CraftingPanelTab::Player, recipe_id);

        assert!(row.button_enabled);
        assert_eq!(row.status, "Craft");
    }

    #[test]
    fn manual_craft_ui_reports_missing_ingredients() {
        let mut sim = Simulation::new_test_world(123);
        let catalog = sim.catalog().clone();
        let iron_plate = item_id_by_name(sim.catalog(), "iron_plate");
        sim.player_inventory_mut()
            .insert(&catalog, iron_plate, 1)
            .expect("test inventory should accept iron plates");
        let recipe_id = recipe_id_by_name(sim.catalog(), "iron_gear_wheel");

        let row = row_for_recipe(&sim, CraftingPanelTab::Player, recipe_id);

        assert!(!row.button_enabled);
        assert_eq!(row.status, "Missing: Iron Plate x1");
        assert!(row.ingredients.contains("Iron Plate 1/2"));
    }

    fn row_for_recipe(
        sim: &Simulation,
        tab: CraftingPanelTab,
        recipe_id: RecipeId,
    ) -> ManualCraftRecipeRow {
        recipe_rows(sim, tab)
            .into_iter()
            .find(|row| row.recipe_id == recipe_id)
            .expect("recipe row should be visible")
    }
}
