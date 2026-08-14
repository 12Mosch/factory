use super::topology_invalidation_ops::{apply_entity_topology_change, impact_for_prototype};
use super::*;

pub(crate) struct EntityRecovery {
    pub(crate) stacks: Vec<ItemStack>,
    pub(crate) bulk_items: Vec<ItemAmount>,
}

pub(crate) fn destroy_to_player_inventory(
    sim: &mut Simulation,
    entity_id: EntityId,
) -> Result<PlacedEntity, EntityDestroyError> {
    let placed = sim
        .entities
        .placed_entity(entity_id)
        .cloned()
        .ok_or(EntityDestroyError::MissingEntity(entity_id))?;
    let recovery = entity_recovery(sim, &placed)?;
    let mut player_inventory = sim.player_inventory.clone();

    for stack in recovery.stacks {
        player_inventory
            .insert_stack(&sim.world.prototypes, stack)
            .map_err(|error| recovery_insert_error(error, stack.item_id()))?;
    }
    for amount in recovery.bulk_items {
        insert_bulk_recovery(&sim.world.prototypes, &mut player_inventory, amount)?;
    }

    // Unlink before removal: the reverse links live on the neighbors, and
    // `remove_placed_entity` only drops this entity's own side.
    sim.unlink_circuit_wires(entity_id);
    // And likewise before the stop's own state goes: the schedules naming it
    // are rewritten while the name it carried can still be read off it.
    sim.forget_train_stop(entity_id);
    let removed = sim
        .entities
        .remove_placed_entity(entity_id)
        .expect("validated placed entity should still be removable");
    sim.unregister_pollution_emitter(entity_id);
    construction_ops::clear_construction_state_for_removed_entity(sim, entity_id);
    sim.player_inventory = player_inventory;
    sim.manual_mining_progress = None;
    let impact = impact_for_prototype(sim, removed.prototype_id);
    apply_entity_topology_change(sim, impact, entity_id, removed.footprint);

    Ok(removed)
}

pub(crate) fn entity_recovery(
    sim: &Simulation,
    placed: &PlacedEntity,
) -> Result<EntityRecovery, EntityDestroyError> {
    let mut stacks = Vec::new();
    stacks.push(
        ItemStack::new(
            &sim.world.prototypes,
            build_item_for_entity(sim, placed.prototype_id)?,
            1,
        )
        .expect("an entity's validated build item should form a valid stack"),
    );
    push_entity_state_recovery_stacks(&sim.world.prototypes, &sim.entities, placed.id, &mut stacks);
    sim.circuit_wire_recovery_stacks(placed.id, &mut stacks);

    let bulk_items = sim
        .entities
        .mining_drill_state(placed.id)
        .ok()
        .and_then(|state| state.pending_output)
        .map(|pending| {
            ItemAmount::new(&sim.world.prototypes, pending.item_id, pending.count)
                .expect("validated pending drill output should form an item amount")
        })
        .into_iter()
        .collect();

    Ok(EntityRecovery { stacks, bulk_items })
}

fn insert_bulk_recovery(
    catalog: &PrototypeCatalog,
    inventory: &mut Inventory,
    amount: ItemAmount,
) -> Result<(), EntityDestroyError> {
    let item_id = amount.item_id();
    let stack_size = catalog
        .item(item_id)
        .ok_or(EntityDestroyError::UnknownItem(item_id))?
        .stack_size;
    if u64::from(inventory.insert_capacity(item_id, stack_size)) < amount.count() {
        return Err(EntityDestroyError::InsufficientInventory { item_id });
    }

    let mut remaining = amount.count();
    while remaining > 0 {
        let chunk = remaining.min(u64::from(u16::MAX)) as u16;
        inventory
            .insert(catalog, item_id, chunk)
            .map_err(|error| recovery_insert_error(error, item_id))?;
        remaining -= u64::from(chunk);
    }
    Ok(())
}

fn recovery_insert_error(error: InventoryError, item_id: ItemId) -> EntityDestroyError {
    match error {
        InventoryError::InsufficientSpace => EntityDestroyError::InsufficientInventory { item_id },
        InventoryError::UnknownItem(item_id) => EntityDestroyError::UnknownItem(item_id),
        InventoryError::InsufficientItems
        | InventoryError::EmptyItemStack(_)
        | InventoryError::StackExceedsLimit { .. }
        | InventoryError::InvalidSlot { .. }
        | InventoryError::EmptySlot { .. }
        | InventoryError::FilterMismatch { .. } => {
            unreachable!("destroy recovery only inserts items")
        }
    }
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
            catalog: &PrototypeCatalog,
            entities: &EntityStore,
            entity_id: EntityId,
            stacks: &mut Vec<ItemStack>,
        ) {
            $(
                if let Some(state) = entities.$field.get(&entity_id) {
                    EntityStateBehavior::push_recovery_stacks(state, catalog, stacks);
                }
            )*
        }
    };
}
for_each_entity_state_map!(define_push_entity_state_recovery_stacks);
