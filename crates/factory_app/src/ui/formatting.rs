use factory_data::{CraftingCategory, FluidId, ItemId, PrototypeCatalog};
use factory_sim::{
    EntityId, ItemStack, RocketSiloOperationalState, RocketSiloStatusDetail, Simulation,
};

use crate::utils::compact_item_name;

pub(crate) fn format_item_stack(stack: ItemStack, catalog: &PrototypeCatalog) -> String {
    let name = catalog
        .item(stack.item_id())
        .map(|item| item.name.as_str())
        .unwrap_or("unknown");
    format!("{}\n{}", compact_item_name(name), stack.count())
}

/// Display label for the product returned by this placed rocket silo.
pub fn format_rocket_silo_launch_product_label(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<String> {
    let placed = sim.entities().placed_entity(entity_id)?;
    sim.catalog().entity(placed.prototype_id)?.rocket_silo?;
    let state = factory_sim::entity_access::rocket_silo_state(sim, entity_id).ok()?;
    let selected = state
        .cargo_inventory
        .slots()
        .first()
        .and_then(|slot| slot.stack())
        .filter(|stack| stack.count() == 1)
        .and_then(|stack| sim.catalog().rocket_launch_products(stack.item_id()));
    let products = selected
        .map(|products| products.iter().collect::<Vec<_>>())
        .unwrap_or_else(|| {
            sim.catalog()
                .rocket_launch_payloads()
                .flat_map(|payload| &payload.launch_products)
                .collect()
        });
    let mut product_items = Vec::new();
    for product in products {
        if !product_items.contains(&product.item) {
            product_items.push(product.item);
        }
    }
    (!product_items.is_empty()).then(|| {
        product_items
            .into_iter()
            .map(|item| format_item_display_name(sim.catalog(), item))
            .collect::<Vec<_>>()
            .join(" / ")
    })
}

/// The four lines a crafting machine's window shows about what it is making.
///
/// Shared by assembling machines and rocket silos: both answer "which recipe,
/// what does it need, what does it make, how far along", and a silo's answers
/// differ only in that the recipe is fixed and the product is a rocket rather
/// than a stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CraftingDetailText {
    pub recipe: String,
    pub ingredients: String,
    pub products: String,
    pub progress: String,
}

impl CraftingDetailText {
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
    machine_recipe_choices(catalog, CraftingCategory::Crafting)
}

pub fn machine_recipe_choices(
    catalog: &PrototypeCatalog,
    category: CraftingCategory,
) -> Vec<&factory_data::RecipePrototype> {
    catalog
        .recipes
        .iter()
        .filter(|recipe| recipe.category == category)
        .collect()
}

pub fn available_crafting_recipe_choices(sim: &Simulation) -> Vec<&factory_data::RecipePrototype> {
    sim.available_recipes(CraftingCategory::Crafting)
}

/// The crafting lines for whichever kind of crafting machine `entity_id` is.
pub fn format_crafting_detail_text(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<CraftingDetailText> {
    format_assembler_detail_text(sim, entity_id)
        .or_else(|| format_rocket_silo_detail_text(sim, entity_id))
}

/// A silo's lines. The recipe is stated rather than chosen, and the "output" is
/// how much of a rocket stands in the silo — the counter is what a player is
/// actually watching, so it takes the place the product stack has elsewhere.
fn format_rocket_silo_detail_text(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<CraftingDetailText> {
    let state = factory_sim::entity_access::rocket_silo_state(sim, entity_id).ok()?;
    let rocket = format!(
        "Output: Rocket {}/{}",
        state.parts_completed, state.parts_per_rocket
    );
    let status = sim.rocket_silo_status_for_entity(entity_id)?;
    let progress = format_rocket_silo_progress(status);
    let Some(recipe) = sim.rocket_silo_recipe() else {
        return Some(CraftingDetailText {
            recipe: "Recipe: <locked>".to_string(),
            ingredients: "Ingredients: <none>".to_string(),
            products: rocket,
            progress,
        });
    };

    let ingredient_lines = recipe
        .ingredients
        .iter()
        .map(|ingredient| {
            let required = u32::from(ingredient.amount);
            let available = state.input_inventory.count(ingredient.item);
            format!(
                "{}: need {}, have {}, missing {}",
                format_item_display_name(sim.catalog(), ingredient.item),
                required,
                available,
                required.saturating_sub(available)
            )
        })
        .collect::<Vec<_>>();

    Some(CraftingDetailText {
        recipe: format!("Recipe: {}", format_recipe_display_name(&recipe.name)),
        ingredients: if ingredient_lines.is_empty() {
            "Ingredients: <none>".to_string()
        } else {
            format!("Ingredients:\n{}", ingredient_lines.join("\n"))
        },
        products: rocket,
        progress,
    })
}

/// Current silo state shown in the machine panel.
pub fn format_rocket_silo_operational_status(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<String> {
    let detail = sim.rocket_silo_status_for_entity(entity_id)?;
    let silo_state = factory_sim::entity_access::rocket_silo_state(sim, entity_id).ok()?;
    let placed = sim.entities().placed_entity(entity_id)?;
    sim.catalog().entity(placed.prototype_id)?.rocket_silo?;

    Some(match detail.state {
        RocketSiloOperationalState::RecipeLocked => {
            "Rocket parts locked — research Rocket Silo to begin.".to_string()
        }
        RocketSiloOperationalState::BuildingParts => format!(
            "Building rocket parts — {}/{} complete.",
            silo_state.parts_completed, silo_state.parts_per_rocket
        ),
        RocketSiloOperationalState::MissingIngredients => {
            "Missing ingredients — add the rocket-part ingredients listed above.".to_string()
        }
        RocketSiloOperationalState::NoPower => {
            "No power — connect the silo to a powered electric network.".to_string()
        }
        RocketSiloOperationalState::AwaitingPayload => {
            let payloads = sim
                .catalog()
                .rocket_launch_payloads()
                .map(|payload| format_item_display_name(sim.catalog(), payload.id))
                .collect::<Vec<_>>()
                .join(" or ");
            format!("Awaiting payload — add 1 {payloads} to the Cargo slot.")
        }
        RocketSiloOperationalState::ReadyToLaunch => {
            "Ready to launch — sequence starts on the next simulation tick.".to_string()
        }
        RocketSiloOperationalState::Sealing => format_launch_state("Sealing rocket", detail),
        RocketSiloOperationalState::Launching => format_launch_state("Launching rocket", detail),
        RocketSiloOperationalState::LaunchOutputBlocked => {
            let products = silo_state
                .cargo_inventory
                .slots()
                .first()
                .and_then(|slot| slot.stack())
                .and_then(|stack| sim.catalog().rocket_launch_products(stack.item_id()))
                .map(|products| {
                    products
                        .iter()
                        .map(|product| {
                            format!(
                                "{} {}",
                                product.amount,
                                format_item_display_name(sim.catalog(), product.item)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" and ")
                })
                .unwrap_or_else(|| "the complete launch reward".to_string());
            format!(
                "Launch output blocked — clear room for {products} in the launch-product slots."
            )
        }
    })
}

fn format_rocket_silo_progress(detail: RocketSiloStatusDetail) -> String {
    match detail.state {
        RocketSiloOperationalState::Sealing => {
            format!("Progress: {}", format_launch_state("Sealing", detail))
        }
        RocketSiloOperationalState::Launching => {
            format!("Progress: {}", format_launch_state("Launching", detail))
        }
        _ => format!(
            "Progress: {}/{}",
            detail.progress_ticks, detail.required_ticks
        ),
    }
}

fn format_launch_state(label: &str, detail: RocketSiloStatusDetail) -> String {
    let percent = detail
        .progress_ticks
        .saturating_mul(100)
        .checked_div(detail.required_ticks)
        .unwrap_or(0);
    format!(
        "{label} — {percent}% ({} ticks remaining)",
        detail.ticks_remaining.unwrap_or(0)
    )
}

fn format_assembler_detail_text(
    sim: &Simulation,
    entity_id: EntityId,
) -> Option<CraftingDetailText> {
    let state = factory_sim::entity_access::assembler_state(sim, entity_id).ok()?;
    let Some(recipe) = state
        .selected_recipe
        .and_then(|recipe_id| sim.catalog().recipe(recipe_id))
    else {
        return Some(CraftingDetailText::empty());
    };

    let statuses = sim.assembler_ingredient_status(entity_id).ok()?;
    let fluid_boxes =
        factory_sim::entity_access::fluid_box_states(sim, entity_id).unwrap_or_default();
    let mut ingredient_lines = statuses
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
        .collect::<Vec<_>>();
    ingredient_lines.extend(recipe.fluid_ingredients.iter().map(|ingredient| {
        let available_milliunits = fluid_boxes
            .iter()
            .filter(|state| state.fluid_id == Some(ingredient.fluid))
            .map(|state| state.amount_milliunits)
            .sum::<u64>();
        format!(
            "{}: need {}, have {}",
            format_fluid_display_name(sim.catalog(), ingredient.fluid),
            ingredient.amount_milliunits / 1000,
            available_milliunits / 1000
        )
    }));
    let ingredients = if ingredient_lines.is_empty() {
        "Ingredients: <none>".to_string()
    } else {
        format!("Ingredients:\n{}", ingredient_lines.join("\n"))
    };

    let product_parts = recipe
        .products
        .iter()
        .map(|product| {
            format!(
                "{} x{}",
                format_item_display_name(sim.catalog(), product.item),
                product.amount
            )
        })
        .chain(recipe.fluid_products.iter().map(|product| {
            format!(
                "{} x{}",
                format_fluid_display_name(sim.catalog(), product.fluid),
                product.amount_milliunits / 1000
            )
        }))
        .collect::<Vec<_>>();
    let products = if product_parts.is_empty() {
        "Output: <none>".to_string()
    } else {
        format!("Output: {}", product_parts.join(", "))
    };

    Some(CraftingDetailText {
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
