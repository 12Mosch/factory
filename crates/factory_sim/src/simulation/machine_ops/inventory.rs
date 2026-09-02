use crate::simulation::*;

/// Runtime-only role attached to an item storage endpoint.
///
/// Policies deliberately do not live in [`ItemSlot`] or serialized state:
/// storage invariants are generic, while acceptance can depend on current
/// recipes, research, and entity prototypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) enum ItemSlotPolicy {
    Unrestricted,
    Fuel,
    FurnaceIngredient,
    AssemblerIngredient(EntityId),
    /// A rocket silo's ingredient slots. Unlike an assembler's these need no
    /// entity id: every silo builds the same part from the same recipe, so what
    /// they accept is a question about the catalog rather than about the machine.
    RocketPartIngredient,
    RocketCargo,
    SciencePack,
    Ammunition(EntityId),
    /// A roboport's robot slots, which take any item declaring a flight
    /// profile — construction and logistic robots alike, since a roboport
    /// stations and charges both the same way.
    Robot,
    /// A roboport's construction-material slots. Every non-robot item is
    /// accepted; robot items are routed to the dedicated robot slots.
    ConstructionMaterial,
    OutputOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::simulation) enum ItemSlotOperation {
    PlayerInsert,
    InserterInsert,
    MachineInsert,
    PlayerExtract,
    InserterExtract,
}

pub(in crate::simulation) fn item_slot_policy_accepts(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    policy: ItemSlotPolicy,
    operation: ItemSlotOperation,
    item_id: ItemId,
) -> bool {
    let rocket_silo_recipe = if policy == ItemSlotPolicy::RocketPartIngredient {
        ResolvedRocketSiloRecipe::new(catalog, research)
    } else {
        ResolvedRocketSiloRecipe::default()
    };
    item_slot_policy_accepts_with_rocket_recipe(
        catalog,
        research,
        entities,
        rocket_silo_recipe,
        policy,
        operation,
        item_id,
    )
}

/// Shared immutable inputs for a bulk item-acceptance pass.
#[derive(Clone, Copy)]
pub(in crate::simulation) struct ItemPolicyContext<'a> {
    pub(in crate::simulation) catalog: &'a PrototypeCatalog,
    pub(in crate::simulation) research: &'a ResearchState,
    pub(in crate::simulation) rocket_silo_recipe: ResolvedRocketSiloRecipe,
}

impl<'a> ItemPolicyContext<'a> {
    pub(in crate::simulation) fn with_rocket_recipe(
        catalog: &'a PrototypeCatalog,
        research: &'a ResearchState,
        rocket_silo_recipe: ResolvedRocketSiloRecipe,
    ) -> Self {
        Self {
            catalog,
            research,
            rocket_silo_recipe,
        }
    }
}

pub(in crate::simulation) fn item_slot_policy_accepts_with_rocket_recipe(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    rocket_silo_recipe: ResolvedRocketSiloRecipe,
    policy: ItemSlotPolicy,
    operation: ItemSlotOperation,
    item_id: ItemId,
) -> bool {
    if !item_slot_policy_allows_operation(policy, operation) {
        return false;
    }
    match operation {
        ItemSlotOperation::PlayerExtract | ItemSlotOperation::InserterExtract => true,
        ItemSlotOperation::PlayerInsert | ItemSlotOperation::InserterInsert => match policy {
            ItemSlotPolicy::Unrestricted => true,
            ItemSlotPolicy::Fuel => fuel_value_joules(catalog, item_id).is_some(),
            ItemSlotPolicy::FurnaceIngredient => {
                furnace_input_accepts_item(catalog, research, item_id)
            }
            ItemSlotPolicy::AssemblerIngredient(entity_id) => {
                let Some(state) = entities.assembling_machines.get(&entity_id) else {
                    return false;
                };
                assembler_input_accepts_item(
                    catalog,
                    research,
                    assembler_machine_category(catalog, entities, entity_id),
                    state,
                    item_id,
                )
            }
            ItemSlotPolicy::RocketPartIngredient => {
                resolved_rocket_silo_input_accepts_item(catalog, rocket_silo_recipe, item_id)
            }
            ItemSlotPolicy::RocketCargo => catalog.rocket_launch_products(item_id).is_some(),
            ItemSlotPolicy::SciencePack => lab_can_accept_item(catalog, item_id),
            ItemSlotPolicy::Ammunition(entity_id) => {
                turret_accepts_ammunition(catalog, entities, entity_id, item_id)
            }
            ItemSlotPolicy::Robot => item_is_robot(catalog, item_id),
            ItemSlotPolicy::ConstructionMaterial => item_is_construction_material(catalog, item_id),
            ItemSlotPolicy::OutputOnly => false,
        },
        ItemSlotOperation::MachineInsert => match policy {
            ItemSlotPolicy::OutputOnly | ItemSlotPolicy::Unrestricted => true,
            ItemSlotPolicy::Fuel => fuel_value_joules(catalog, item_id).is_some(),
            ItemSlotPolicy::FurnaceIngredient => catalog.recipes.iter().any(|recipe| {
                recipe.category == CraftingCategory::Smelting
                    && recipe
                        .ingredients
                        .iter()
                        .any(|ingredient| ingredient.item == item_id)
            }),
            ItemSlotPolicy::AssemblerIngredient(entity_id) => {
                let Some(state) = entities.assembling_machines.get(&entity_id) else {
                    return false;
                };
                let machine_category = assembler_machine_category(catalog, entities, entity_id);
                state
                    .selected_recipe
                    .and_then(|recipe_id| catalog.recipe(recipe_id))
                    .is_some_and(|recipe| {
                        recipe.category == machine_category
                            && recipe
                                .ingredients
                                .iter()
                                .any(|ingredient| ingredient.item == item_id)
                    })
            }
            ItemSlotPolicy::RocketPartIngredient => {
                resolved_rocket_silo_input_accepts_item(catalog, rocket_silo_recipe, item_id)
            }
            ItemSlotPolicy::RocketCargo => catalog.rocket_launch_products(item_id).is_some(),
            ItemSlotPolicy::SciencePack => lab_can_accept_item(catalog, item_id),
            ItemSlotPolicy::Ammunition(entity_id) => {
                turret_accepts_ammunition(catalog, entities, entity_id, item_id)
            }
            ItemSlotPolicy::Robot => item_is_robot(catalog, item_id),
            ItemSlotPolicy::ConstructionMaterial => item_is_construction_material(catalog, item_id),
        },
    }
}

/// Whether an item matches the ammunition category declared by a gun turret.
fn turret_accepts_ammunition(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    entity_id: EntityId,
    item_id: ItemId,
) -> bool {
    let accepted_category = entities
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
        .and_then(|prototype| prototype.gun_turret)
        .map(|turret| turret.ammo_category);
    let item_category = catalog
        .item(item_id)
        .and_then(|item| item.ammo)
        .map(|ammo| ammo.category);
    accepted_category.is_some() && accepted_category == item_category
}

/// Whether an item is a robot a roboport can station.
///
/// The flight profile is what makes an item a robot, so this and the dispatch
/// path read the same field: nothing can sit in the robot slots that could not
/// be sent out.
fn item_is_robot(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog
        .item(item_id)
        .is_some_and(|item| item.robot.is_some())
}

/// Whether an item is construction material a roboport stocks. Robot items are
/// kept out so every insertion route agrees which inventory owns them.
fn item_is_construction_material(catalog: &PrototypeCatalog, item_id: ItemId) -> bool {
    catalog
        .item(item_id)
        .is_some_and(|item| item.robot.is_none())
}

pub(in crate::simulation) fn item_slot_policy_allows_operation(
    policy: ItemSlotPolicy,
    operation: ItemSlotOperation,
) -> bool {
    match operation {
        ItemSlotOperation::PlayerExtract => true,
        // Roboport contents are stocked, never harvested: an inserter that
        // could pull robots back out would fight the network for them.
        ItemSlotOperation::InserterExtract => matches!(
            policy,
            ItemSlotPolicy::Unrestricted | ItemSlotPolicy::SciencePack | ItemSlotPolicy::OutputOnly
        ),
        ItemSlotOperation::PlayerInsert | ItemSlotOperation::InserterInsert => {
            policy != ItemSlotPolicy::OutputOnly
        }
        ItemSlotOperation::MachineInsert => true,
    }
}

pub(in crate::simulation) fn item_slot_can_accept(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    policy: ItemSlotPolicy,
    operation: ItemSlotOperation,
    slot: ItemSlot,
    stack: ItemStack,
) -> bool {
    let rocket_silo_recipe = if policy == ItemSlotPolicy::RocketPartIngredient {
        ResolvedRocketSiloRecipe::new(catalog, research)
    } else {
        ResolvedRocketSiloRecipe::default()
    };
    item_slot_policy_accepts_with_rocket_recipe(
        catalog,
        research,
        entities,
        rocket_silo_recipe,
        policy,
        operation,
        stack.item_id(),
    ) && slot.can_insert(catalog, stack)
}

pub(in crate::simulation) fn inventory_policy_for_entity(
    entities: &EntityStore,
    entity_id: EntityId,
) -> ItemSlotPolicy {
    if entities.labs.contains_key(&entity_id) {
        ItemSlotPolicy::SciencePack
    } else if entities.gun_turrets.contains_key(&entity_id) {
        ItemSlotPolicy::Ammunition(entity_id)
    } else {
        ItemSlotPolicy::Unrestricted
    }
}
