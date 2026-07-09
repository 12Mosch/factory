use super::topology_invalidation_ops::{apply_entity_topology_change, impact_for_prototype};
use super::*;

pub(crate) fn destroy_to_player_inventory(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<PlacedEntity, EntityDestroyError> {
    let placed = sim
        .entities
        .placed_entity(entity_id)
        .cloned()
        .ok_or(EntityDestroyError::MissingEntity(entity_id))?;
    let recovery_stacks = entity_recovery_stacks(sim, &placed)?;
    let mut player_inventory = sim.player_inventory.clone();

    for stack in recovery_stacks {
        player_inventory
            .insert(&sim.world.prototypes, stack.item_id, stack.count)
            .map_err(|error| match error {
                InventoryError::InsufficientSpace => EntityDestroyError::InsufficientInventory {
                    item_id: stack.item_id,
                },
                InventoryError::UnknownItem => EntityDestroyError::UnknownItem(stack.item_id),
                InventoryError::InsufficientItems => {
                    unreachable!("destroy recovery only inserts items")
                }
            })?;
    }

    let removed = sim
        .entities
        .remove_placed_entity(entity_id)
        .expect("validated placed entity should still be removable");
    construction_ops::clear_construction_state_for_removed_entity(sim, entity_id);
    sim.player_inventory = player_inventory;
    sim.manual_mining_progress = None;
    let impact = impact_for_prototype(sim, removed.prototype_id);
    apply_entity_topology_change(sim, impact);

    Ok(removed)
}

pub(crate) fn entity_recovery_stacks(
    sim: &Simulation,
    placed: &PlacedEntity,
) -> Result<Vec<ItemStack>, EntityDestroyError> {
    let mut stacks = Vec::new();
    stacks.push(ItemStack {
        item_id: build_item_for_entity(sim, placed.prototype_id)?,
        count: 1,
    });
    push_entity_state_recovery_stacks(&sim.entities, placed.id, &mut stacks);

    Ok(stacks)
}

pub(crate) fn build_item_for_entity(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
) -> Result<ItemId, EntityDestroyError> {
    let prototype = sim
        .world
        .prototypes
        .entity(prototype_id)
        .ok_or(EntityDestroyError::MissingBuildItem { prototype_id })?;

    let build_item = prototype
        .build_item
        .ok_or(EntityDestroyError::MissingBuildItem { prototype_id })?;

    sim.world
        .prototypes
        .item(build_item)
        .map(|item| item.id)
        .ok_or(EntityDestroyError::MissingBuildItem { prototype_id })
}

macro_rules! define_push_entity_state_recovery_stacks {
    ($($field:ident : $ty:ty => $kind:tt),* $(,)?) => {
        /// Collects the items recovered from every state entry owned by
        /// `entity_id` when the entity is destroyed.
        pub(crate) fn push_entity_state_recovery_stacks(
            entities: &EntityStore,
            entity_id: EntityId,
            stacks: &mut Vec<ItemStack>,
        ) {
            $(
                if let Some(state) = entities.$field.get(&entity_id) {
                    EntityStateBehavior::push_recovery_stacks(state, stacks);
                }
            )*
        }
    };
}
for_each_entity_state_map!(define_push_entity_state_recovery_stacks);
