use crate::simulation::*;

pub(in crate::simulation) fn inserter_transfer_tiles(
    catalog: &PrototypeCatalog,
    placed: &PlacedEntity,
) -> Option<((i32, i32), (i32, i32))> {
    let prototype = catalog.entity(placed.prototype_id)?;
    let inserter = prototype.inserter.as_ref()?;

    Some(inserter_transfer_tiles_for_prototype(placed, inserter))
}

pub(in crate::simulation) fn inserter_transfer_tiles_for_prototype(
    placed: &PlacedEntity,
    inserter: &factory_data::InserterPrototype,
) -> ((i32, i32), (i32, i32)) {
    let pickup_offset = rotate_inserter_offset(
        (inserter.pickup_offset.x, inserter.pickup_offset.y),
        placed.direction,
    );
    let drop_offset = rotate_inserter_offset(
        (inserter.drop_offset.x, inserter.drop_offset.y),
        placed.direction,
    );

    (
        (placed.x + pickup_offset.0, placed.y + pickup_offset.1),
        (placed.x + drop_offset.0, placed.y + drop_offset.1),
    )
}

fn rotate_inserter_offset(offset: (i32, i32), direction: Direction) -> (i32, i32) {
    let (x, y) = offset;
    match direction {
        Direction::North => (x, y),
        Direction::East => (y, -x),
        Direction::South => (-x, -y),
        Direction::West => (-y, x),
    }
}

pub(in crate::simulation) fn peek_inserter_source_item(
    entities: &EntityStore,
    pickup_tile: (i32, i32),
) -> Option<ItemId> {
    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.item_id)
            .next();
    }

    if let Some(lab) = entities.labs.get(&entity_id) {
        return lab
            .inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.item_id)
            .next();
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return furnace.output_slot.map(|stack| stack.item_id);
    }

    if let Some(assembler) = entities.assembling_machines.get(&entity_id) {
        return assembler
            .output_inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| stack.item_id)
            .next();
    }

    entities
        .transport_belts
        .get(&entity_id)
        .and_then(belt_pickup_item)
        .or_else(|| {
            entities
                .splitters
                .get(&entity_id)
                .and_then(splitter_pickup_item)
        })
}

pub(in crate::simulation) fn inserter_target_can_accept(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    drop_tile: (i32, i32),
    item: ItemStack,
) -> bool {
    let Some(entity_id) = entities.occupancy.entity_at(drop_tile.0, drop_tile.1) else {
        return false;
    };

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return inventory.can_insert(catalog, item.item_id, item.count);
    }

    if let Some(lab) = entities.labs.get(&entity_id) {
        return lab_can_accept_item(catalog, item.item_id)
            && lab.inventory.can_insert(catalog, item.item_id, item.count);
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return burner_fuel_slot_can_accept(catalog, furnace.energy.fuel_slot, item)
            || input_slot_can_accept(catalog, research, furnace.input_slot, item);
    }

    if let Some(boiler) = entities.boilers.get(&entity_id) {
        return burner_fuel_slot_can_accept(catalog, boiler.energy.fuel_slot, item);
    }

    if let Some(assembler) = entities.assembling_machines.get(&entity_id) {
        return assembler_input_can_accept(catalog, research, assembler, item)
            && assembler
                .input_inventory
                .can_insert(catalog, item.item_id, item.count);
    }

    entities
        .transport_belts
        .get(&entity_id)
        .is_some_and(|segment| {
            item.count == 1 && belt_output_lane_index(segment, item.item_id).is_some()
        })
        || entities.splitters.get(&entity_id).is_some_and(|state| {
            let Some(input_port) =
                splitter_input_port_for_occupied_tile(entities, entity_id, drop_tile)
            else {
                return false;
            };
            item.count == 1 && splitter_output_lane_index(state, input_port, item.item_id).is_some()
        })
}

pub(in crate::simulation) fn try_take_inserter_source_item(
    entities: &mut EntityStore,
    pickup_tile: (i32, i32),
    item_id: ItemId,
) -> Option<ItemStack> {
    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get_mut(&entity_id) {
        inventory.remove(item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(lab) = entities.labs.get_mut(&entity_id) {
        lab.inventory.remove(item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(furnace) = entities.furnaces.get_mut(&entity_id) {
        remove_from_single_slot(&mut furnace.output_slot, item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(assembler) = entities.assembling_machines.get_mut(&entity_id) {
        assembler.output_inventory.remove(item_id, 1).ok()?;
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(segment) = entities.transport_belts.get_mut(&entity_id)
        && remove_one_item_from_belt(segment, item_id)
    {
        return Some(ItemStack { item_id, count: 1 });
    }

    if let Some(state) = entities.splitters.get_mut(&entity_id)
        && remove_one_item_from_splitter(state, item_id)
    {
        return Some(ItemStack { item_id, count: 1 });
    }

    None
}

pub(in crate::simulation) fn try_drop_inserter_item(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &mut EntityStore,
    drop_tile: (i32, i32),
    item: ItemStack,
) -> bool {
    let Some(entity_id) = entities.occupancy.entity_at(drop_tile.0, drop_tile.1) else {
        return false;
    };

    if let Some(inventory) = entities.entity_inventories.get_mut(&entity_id) {
        return inventory.insert(catalog, item.item_id, item.count).is_ok();
    }

    if let Some(lab) = entities.labs.get_mut(&entity_id) {
        if !lab_can_accept_item(catalog, item.item_id) {
            return false;
        }

        return lab
            .inventory
            .insert(catalog, item.item_id, item.count)
            .is_ok();
    }

    if let Some(furnace) = entities.furnaces.get_mut(&entity_id) {
        if burner_fuel_slot_can_accept(catalog, furnace.energy.fuel_slot, item) {
            insert_into_single_slot(&mut furnace.energy.fuel_slot, item);
            return true;
        }

        if input_slot_can_accept(catalog, research, furnace.input_slot, item) {
            insert_into_single_slot(&mut furnace.input_slot, item);
            return true;
        }

        return false;
    }

    if let Some(boiler) = entities.boilers.get_mut(&entity_id) {
        if burner_fuel_slot_can_accept(catalog, boiler.energy.fuel_slot, item) {
            insert_into_single_slot(&mut boiler.energy.fuel_slot, item);
            return true;
        }

        return false;
    }

    if let Some(assembler) = entities.assembling_machines.get_mut(&entity_id) {
        if !assembler_input_can_accept(catalog, research, assembler, item) {
            return false;
        }

        return assembler
            .input_inventory
            .insert(catalog, item.item_id, item.count)
            .is_ok();
    }

    if let Some(segment) = entities.transport_belts.get_mut(&entity_id) {
        if item.count != 1 {
            return false;
        }

        let Some(lane_index) = belt_output_lane_index(segment, item.item_id) else {
            return false;
        };
        segment.lanes[lane_index].items.insert(
            0,
            BeltItem {
                item_id: item.item_id,
                position_subtile: 0,
            },
        );
        return true;
    }

    let splitter_input_port = splitter_input_port_for_occupied_tile(entities, entity_id, drop_tile);
    if let Some(state) = entities.splitters.get_mut(&entity_id) {
        if item.count != 1 {
            return false;
        }

        let Some(input_port) = splitter_input_port else {
            return false;
        };
        let Some(lane_index) = splitter_output_lane_index(state, input_port, item.item_id) else {
            return false;
        };
        state.input_lanes[input_port][lane_index].items.insert(
            0,
            BeltItem {
                item_id: item.item_id,
                position_subtile: 0,
            },
        );
        return true;
    }

    false
}
