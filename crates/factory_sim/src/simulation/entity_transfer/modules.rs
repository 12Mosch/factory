use super::*;
fn module_slots_mut(
    entities: &mut EntityStore,
    entity_id: EntityId,
) -> Result<&mut ModuleSlots, ModuleError> {
    let is_placed = entities.placed_entity(entity_id).is_some();
    if let Some(slots) = entities.module_slots_mut(entity_id) {
        Ok(slots)
    } else if is_placed {
        Err(ModuleError::UnsupportedMachine(entity_id))
    } else {
        Err(ModuleError::MissingEntity(entity_id))
    }
}

pub(super) fn player_slot_to_modules(
    sim: &mut Simulation,
    entity_id: EntityId,
    player_slot_index: usize,
) -> Result<TransferOutcome, ModuleError> {
    let stack = sim
        .player_inventory
        .slot(player_slot_index)
        .ok_or(ModuleError::EmptySlot {
            slot_index: player_slot_index,
        })?;
    if sim
        .world
        .prototypes
        .item(stack.item_id())
        .is_none_or(|item| item.module_effect.is_none())
    {
        return Err(ModuleError::InvalidModule(stack.item_id()));
    }
    let free_slots = entity_access::module_slots(sim, entity_id)?
        .slots()
        .iter()
        .filter(|slot| slot.is_empty())
        .count();
    if entity_access::module_slots(sim, entity_id)?.is_empty() {
        return Err(ModuleError::UnsupportedMachine(entity_id));
    }
    if free_slots == 0 {
        return Err(ModuleError::InsufficientSpace);
    }
    let moved_quantity = usize::from(stack.count()).min(free_slots) as u16;
    {
        let catalog = &sim.world.prototypes;
        let slots = module_slots_mut(&mut sim.entities, entity_id)?;
        let mut remaining = moved_quantity;
        for slot_index in 0..slots.len() {
            if remaining == 0 {
                break;
            }
            let slot = slots.slot_mut(slot_index).expect("index is in bounds");
            if slot.is_empty() {
                slot.insert(catalog, stack.item_id(), 1)
                    .expect("validated module should fit an empty slot");
                remaining -= 1;
            }
        }
    }
    sim.player_inventory
        .remove(stack.item_id(), moved_quantity)
        .expect("planned player module quantity remains available");
    refresh_after_module_transfer(sim, entity_id);
    Ok(TransferOutcome { moved_quantity })
}

pub(super) fn module_slot_to_player(
    sim: &mut Simulation,
    entity_id: EntityId,
    slot_index: usize,
) -> Result<TransferOutcome, ModuleError> {
    let stack = entity_access::module_slots(sim, entity_id)?
        .slot(slot_index)
        .ok_or_else(|| {
            if slot_index >= entity_access::module_slots(sim, entity_id).map_or(0, ModuleSlots::len)
            {
                ModuleError::InvalidSlot { slot_index }
            } else {
                ModuleError::EmptySlot { slot_index }
            }
        })?;
    if sim.player_inventory.insert_capacity(
        stack.item_id(),
        sim.world
            .prototypes
            .item(stack.item_id())
            .map_or(1, |item| item.stack_size),
    ) == 0
    {
        return Err(ModuleError::InsufficientSpace);
    }
    sim.player_inventory
        .insert(&sim.world.prototypes, stack.item_id(), 1)
        .expect("checked player inventory capacity");
    module_slots_mut(&mut sim.entities, entity_id)?
        .slot_mut(slot_index)
        .expect("validated module slot remains in bounds")
        .remove(stack.item_id(), 1)
        .expect("validated occupied module slot remains removable");
    refresh_after_module_transfer(sim, entity_id);
    Ok(TransferOutcome { moved_quantity: 1 })
}

fn refresh_after_module_transfer(sim: &mut Simulation, entity_id: EntityId) {
    if sim.entities.beacons.contains_key(&entity_id) {
        sim.refresh_machines_covered_by_beacon(entity_id);
    } else {
        sim.refresh_module_effects(entity_id);
    }
}
