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
    SciencePack,
    Ammunition,
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
            ItemSlotPolicy::SciencePack => lab_can_accept_item(catalog, item_id),
            ItemSlotPolicy::Ammunition => item_is_ammo(catalog, item_id),
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
            ItemSlotPolicy::SciencePack => lab_can_accept_item(catalog, item_id),
            ItemSlotPolicy::Ammunition => item_is_ammo(catalog, item_id),
            ItemSlotPolicy::Robot => item_is_robot(catalog, item_id),
            ItemSlotPolicy::ConstructionMaterial => item_is_construction_material(catalog, item_id),
        },
    }
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
    item_slot_policy_accepts(
        catalog,
        research,
        entities,
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
        ItemSlotPolicy::Ammunition
    } else {
        ItemSlotPolicy::Unrestricted
    }
}
