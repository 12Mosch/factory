use crate::simulation::entity_transfer::RoboportInventory;
use crate::simulation::rolling_stock_ops::{
    StoppedStock, StoppedStockMut, drop_stock_item, stock_can_accept, stock_pickup_item,
    take_stock_item,
};
use crate::simulation::*;

/// Which of a roboport's two inventories would take `item` from an inserter, or
/// `None` when neither will.
///
/// The two inventories accept disjoint item sets, so this both answers "can the
/// inserter drop here?" and picks the destination — one function rather than a
/// predicate and a separate router that could disagree.
fn roboport_inventory_accepting(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    roboport: &RoboportState,
    item: ItemStack,
) -> Option<RoboportInventory> {
    let candidates = [
        (
            RoboportInventory::Robots,
            ItemSlotPolicy::Robot,
            &roboport.robots,
        ),
        (
            RoboportInventory::Materials,
            ItemSlotPolicy::ConstructionMaterial,
            &roboport.materials,
        ),
    ];
    candidates
        .into_iter()
        .find(|(_, policy, inventory)| {
            item_slot_policy_accepts(
                catalog,
                research,
                entities,
                *policy,
                ItemSlotOperation::InserterInsert,
                item.item_id(),
            ) && inventory.can_insert(catalog, item.item_id(), item.count())
        })
        .map(|(target, _, _)| target)
}

pub(in crate::simulation) fn inserter_transfer_tiles(
    catalog: &PrototypeCatalog,
    placed: &PlacedEntity,
) -> Option<(
    (WorldTileCoord, WorldTileCoord),
    (WorldTileCoord, WorldTileCoord),
)> {
    let prototype = catalog.entity(placed.prototype_id)?;
    let inserter = prototype.inserter.as_ref()?;

    Some(inserter_transfer_tiles_for_prototype(placed, inserter))
}

pub(in crate::simulation) fn inserter_transfer_tiles_for_prototype(
    placed: &PlacedEntity,
    inserter: &factory_data::InserterPrototype,
) -> (
    (WorldTileCoord, WorldTileCoord),
    (WorldTileCoord, WorldTileCoord),
) {
    let pickup_offset = rotate_inserter_offset(
        (inserter.pickup_offset.x, inserter.pickup_offset.y),
        placed.direction,
    );
    let drop_offset = rotate_inserter_offset(
        (inserter.drop_offset.x, inserter.drop_offset.y),
        placed.direction,
    );

    (
        (
            placed.x + i64::from(pickup_offset.0),
            placed.y + i64::from(pickup_offset.1),
        ),
        (
            placed.x + i64::from(drop_offset.0),
            placed.y + i64::from(drop_offset.1),
        ),
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
    stopped_stock: StoppedStock<'_>,
    pickup_tile: (WorldTileCoord, WorldTileCoord),
) -> Option<ItemId> {
    // Stopped stock first. A wagon stands on a rail, and the rail is an
    // ordinary placed entity, so the occupancy lookup below would answer with a
    // rail piece — an entity that holds nothing — and never look further. The
    // wagon is the more specific answer for that tile, and the only one that
    // holds items.
    if let Some(stock) = stopped_stock.at(pickup_tile.0, pickup_tile.1) {
        return stock_pickup_item(stock);
    }

    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return inventory
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
            .next();
    }

    if let Some(lab) = entities.labs.get(&entity_id) {
        return lab
            .inventory
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
            .next();
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return furnace.output_slot.stack().map(|stack| stack.item_id());
    }

    // Only spent fuel cells leave a reactor; the fuel slot is an input.
    if let Some(reactor) = entities.nuclear_reactors.get(&entity_id) {
        return reactor.output_slot.stack().map(|stack| stack.item_id());
    }

    if let Some(assembler) = entities.assembling_machines.get(&entity_id) {
        return assembler
            .output_inventory
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
            .next();
    }

    if let Some(silo) = entities.rocket_silos.get(&entity_id) {
        return silo
            .output_inventory
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
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
    stopped_stock: StoppedStock<'_>,
    drop_tile: (WorldTileCoord, WorldTileCoord),
    item: ItemStack,
) -> bool {
    if let Some(stock) = stopped_stock.at(drop_tile.0, drop_tile.1) {
        return stock_can_accept(catalog, research, entities, stock, item);
    }

    let Some(entity_id) = entities.occupancy.entity_at(drop_tile.0, drop_tile.1) else {
        return false;
    };

    if let Some(inventory) = entities.entity_inventories.get(&entity_id) {
        return item_slot_policy_accepts(
            catalog,
            research,
            entities,
            inventory_policy_for_entity(entities, entity_id),
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) && inventory.can_insert(catalog, item.item_id(), item.count());
    }

    if let Some(lab) = entities.labs.get(&entity_id) {
        return item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::SciencePack,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) && lab
            .inventory
            .can_insert(catalog, item.item_id(), item.count());
    }

    if let Some(turret) = entities.gun_turrets.get(&entity_id) {
        return item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Ammunition,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) && turret
            .ammo
            .can_insert(catalog, item.item_id(), item.count());
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        return furnace.energy.fuel_slot().is_some_and(|fuel_slot| {
            item_slot_can_accept(
                catalog,
                research,
                entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::InserterInsert,
                fuel_slot,
                item,
            )
        }) || item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::FurnaceIngredient,
            ItemSlotOperation::InserterInsert,
            furnace.input_slot,
            item,
        );
    }

    if let Some(boiler) = entities.boilers.get(&entity_id) {
        return item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            boiler.energy.fuel_slot,
            item,
        );
    }

    if let Some(reactor) = entities.nuclear_reactors.get(&entity_id) {
        return item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            reactor.energy.fuel_slot,
            item,
        );
    }

    if let Some(roboport) = entities.roboports.get(&entity_id) {
        return roboport_inventory_accepting(catalog, research, entities, roboport, item).is_some();
    }

    if let Some(fuel_slot) = entities
        .inserter_energy
        .get(&entity_id)
        .and_then(MachineEnergy::fuel_slot)
    {
        return item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            fuel_slot,
            item,
        );
    }

    if let Some(assembler) = entities.assembling_machines.get(&entity_id) {
        return item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::AssemblerIngredient(entity_id),
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) && assembler
            .input_inventory
            .can_insert(catalog, item.item_id(), item.count());
    }

    // A completed rocket routes its one launchable payload to the cargo slot;
    // at every other time inserters continue stocking part ingredients.
    if let Some(silo) = entities.rocket_silos.get(&entity_id) {
        let is_cargo =
            rocket_silo_cargo_target_accepts(catalog, research, entities, entity_id, item);
        if is_cargo {
            return true;
        }
        return item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::RocketPartIngredient,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) && silo
            .input_inventory
            .can_insert(catalog, item.item_id(), item.count());
    }

    entities
        .transport_belts
        .get(&entity_id)
        .is_some_and(|segment| {
            item.count() == 1 && belt_output_lane_index(segment, item.item_id()).is_some()
        })
        || entities.splitters.get(&entity_id).is_some_and(|state| {
            let Some(input_port) =
                splitter_input_port_for_occupied_tile(entities, entity_id, drop_tile)
            else {
                return false;
            };
            item.count() == 1
                && splitter_output_lane_index(state, input_port, item.item_id()).is_some()
        })
}

pub(in crate::simulation) fn try_take_inserter_source_item(
    catalog: &PrototypeCatalog,
    entities: &mut EntityStore,
    stopped_stock: &mut StoppedStockMut<'_>,
    transport: &mut TransportLaneCache,
    pickup_tile: (WorldTileCoord, WorldTileCoord),
    item_id: ItemId,
) -> Option<ItemStack> {
    if let Some(stock) = stopped_stock.at_mut(pickup_tile.0, pickup_tile.1) {
        return take_stock_item(catalog, stock, item_id);
    }

    let entity_id = entities.occupancy.entity_at(pickup_tile.0, pickup_tile.1)?;

    if let Some(inventory) = entities.chest_inventory_mut(entity_id) {
        if !item_slot_policy_allows_operation(
            ItemSlotPolicy::Unrestricted,
            ItemSlotOperation::InserterExtract,
        ) {
            return None;
        }
        inventory.remove(item_id, 1).ok()?;
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(lab) = entities.labs.get_mut(&entity_id) {
        if !item_slot_policy_allows_operation(
            ItemSlotPolicy::SciencePack,
            ItemSlotOperation::InserterExtract,
        ) {
            return None;
        }
        lab.inventory.remove(item_id, 1).ok()?;
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(furnace) = entities.furnaces.get_mut(&entity_id) {
        if !item_slot_policy_allows_operation(
            ItemSlotPolicy::OutputOnly,
            ItemSlotOperation::InserterExtract,
        ) {
            return None;
        }
        furnace.output_slot.remove(item_id, 1).ok()?;
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(reactor) = entities.nuclear_reactors.get_mut(&entity_id) {
        if !item_slot_policy_allows_operation(
            ItemSlotPolicy::OutputOnly,
            ItemSlotOperation::InserterExtract,
        ) {
            return None;
        }
        reactor.output_slot.remove(item_id, 1).ok()?;
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(assembler) = entities.assembling_machines.get_mut(&entity_id) {
        if !item_slot_policy_allows_operation(
            ItemSlotPolicy::OutputOnly,
            ItemSlotOperation::InserterExtract,
        ) {
            return None;
        }
        assembler.output_inventory.remove(item_id, 1).ok()?;
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(silo) = entities.rocket_silos.get_mut(&entity_id) {
        if !item_slot_policy_allows_operation(
            ItemSlotPolicy::OutputOnly,
            ItemSlotOperation::InserterExtract,
        ) {
            return None;
        }
        silo.output_inventory.remove(item_id, 1).ok()?;
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(segment) = entities.transport_belts.get_mut(&entity_id)
        && let Some(lane_index) = remove_one_item_from_belt(segment, item_id)
    {
        transport.mark_items_changed(entity_id);
        transport.mark_active_with_upstreams(TransportLaneKey::Belt {
            entity_id,
            lane_index,
        });
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    if let Some(state) = entities.splitters.get_mut(&entity_id)
        && let Some((input_port, lane_index)) = remove_one_item_from_splitter(state, item_id)
    {
        transport.mark_items_changed(entity_id);
        transport.mark_active_with_upstreams(TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        });
        return Some(
            ItemStack::new(catalog, item_id, 1)
                .expect("a removed inserter source item should form a valid stack"),
        );
    }

    None
}

pub(in crate::simulation) fn try_drop_inserter_item(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &mut EntityStore,
    stopped_stock: &mut StoppedStockMut<'_>,
    transport: &mut TransportLaneCache,
    drop_tile: (WorldTileCoord, WorldTileCoord),
    item: ItemStack,
) -> bool {
    if let Some(stock) = stopped_stock.at_mut(drop_tile.0, drop_tile.1) {
        return drop_stock_item(catalog, research, entities, stock, item);
    }

    let Some(entity_id) = entities.occupancy.entity_at(drop_tile.0, drop_tile.1) else {
        return false;
    };

    if entities.entity_inventories.contains_key(&entity_id) {
        if !item_slot_policy_accepts(
            catalog,
            research,
            entities,
            inventory_policy_for_entity(entities, entity_id),
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) {
            return false;
        }
        let inventory = entities
            .chest_inventory_mut(entity_id)
            .expect("inventory presence was checked above");
        return inventory
            .insert(catalog, item.item_id(), item.count())
            .is_ok();
    }

    if entities.labs.contains_key(&entity_id) {
        if !item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::SciencePack,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) {
            return false;
        }
        let lab = entities
            .labs
            .get_mut(&entity_id)
            .expect("lab presence was checked above");
        return lab
            .inventory
            .insert(catalog, item.item_id(), item.count())
            .is_ok();
    }

    if entities.gun_turrets.contains_key(&entity_id) {
        if !item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Ammunition,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) {
            return false;
        }
        let turret = entities
            .gun_turrets
            .get_mut(&entity_id)
            .expect("turret presence was checked above");
        return turret
            .ammo
            .insert(catalog, item.item_id(), item.count())
            .is_ok();
    }

    if let Some(furnace) = entities.furnaces.get(&entity_id) {
        let fuel_accepts = furnace.energy.fuel_slot().is_some_and(|fuel_slot| {
            item_slot_can_accept(
                catalog,
                research,
                entities,
                ItemSlotPolicy::Fuel,
                ItemSlotOperation::InserterInsert,
                fuel_slot,
                item,
            )
        });
        let input_accepts = !fuel_accepts
            && item_slot_can_accept(
                catalog,
                research,
                entities,
                ItemSlotPolicy::FurnaceIngredient,
                ItemSlotOperation::InserterInsert,
                furnace.input_slot,
                item,
            );
        let furnace = entities
            .furnaces
            .get_mut(&entity_id)
            .expect("furnace presence was checked above");
        if fuel_accepts {
            furnace
                .energy
                .fuel_slot_mut()
                .expect("an accepting furnace fuel slot exists")
                .insert_stack(catalog, item)
                .expect("the checked furnace fuel slot should accept the item");
            return true;
        }

        if input_accepts {
            furnace
                .input_slot
                .insert_stack(catalog, item)
                .expect("the checked furnace input slot should accept the item");
            return true;
        }

        return false;
    }

    if let Some(boiler) = entities.boilers.get(&entity_id) {
        let fuel_accepts = item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            boiler.energy.fuel_slot,
            item,
        );
        let boiler = entities
            .boilers
            .get_mut(&entity_id)
            .expect("boiler presence was checked above");
        if fuel_accepts {
            boiler
                .energy
                .fuel_slot
                .insert_stack(catalog, item)
                .expect("the checked boiler fuel slot should accept the item");
            return true;
        }

        return false;
    }

    if let Some(reactor) = entities.nuclear_reactors.get(&entity_id) {
        let fuel_accepts = item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            reactor.energy.fuel_slot,
            item,
        );
        if !fuel_accepts {
            return false;
        }
        entities
            .nuclear_reactors
            .get_mut(&entity_id)
            .expect("reactor presence was checked above")
            .energy
            .fuel_slot
            .insert_stack(catalog, item)
            .expect("the checked reactor fuel slot should accept the item");
        return true;
    }

    if let Some(roboport) = entities.roboports.get(&entity_id) {
        let Some(target) =
            roboport_inventory_accepting(catalog, research, entities, roboport, item)
        else {
            return false;
        };
        let roboport = entities
            .roboports
            .get_mut(&entity_id)
            .expect("roboport presence was checked above");
        let inventory = match target {
            RoboportInventory::Robots => &mut roboport.robots,
            RoboportInventory::Materials => &mut roboport.materials,
        };
        inventory
            .insert_stack(catalog, item)
            .expect("the checked roboport inventory should accept the item");
        return true;
    }

    if let Some(fuel_slot) = entities
        .inserter_energy
        .get(&entity_id)
        .and_then(MachineEnergy::fuel_slot)
    {
        let fuel_accepts = item_slot_can_accept(
            catalog,
            research,
            entities,
            ItemSlotPolicy::Fuel,
            ItemSlotOperation::InserterInsert,
            fuel_slot,
            item,
        );
        if !fuel_accepts {
            return false;
        }
        entities
            .inserter_energy
            .get_mut(&entity_id)
            .and_then(MachineEnergy::fuel_slot_mut)
            .expect("an accepting burner inserter fuel slot exists")
            .insert_stack(catalog, item)
            .expect("the checked burner inserter fuel slot should accept the item");
        return true;
    }

    if entities.assembling_machines.contains_key(&entity_id) {
        let accepts = item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::AssemblerIngredient(entity_id),
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        );
        let assembler = entities
            .assembling_machines
            .get_mut(&entity_id)
            .expect("assembler presence was checked above");
        if !accepts {
            return false;
        }

        return assembler
            .input_inventory
            .insert(catalog, item.item_id(), item.count())
            .is_ok();
    }

    if entities.rocket_silos.contains_key(&entity_id) {
        let cargo = rocket_silo_cargo_target_accepts(catalog, research, entities, entity_id, item);
        let policy = if cargo {
            ItemSlotPolicy::RocketCargo
        } else {
            ItemSlotPolicy::RocketPartIngredient
        };
        if !item_slot_policy_accepts(
            catalog,
            research,
            entities,
            policy,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        ) {
            return false;
        }
        let inserted = {
            let silo = entities
                .rocket_silos
                .get_mut(&entity_id)
                .expect("rocket silo presence was checked above");
            let inventory = if cargo {
                &mut silo.cargo_inventory
            } else {
                &mut silo.input_inventory
            };
            inventory
                .insert(catalog, item.item_id(), item.count())
                .is_ok()
        };
        if inserted && cargo {
            entities.note_logistic_endpoint_changed(entity_id);
        }
        return inserted;
    }

    if let Some(segment) = entities.transport_belts.get_mut(&entity_id) {
        if item.count() != 1 {
            return false;
        }

        let Some(lane_index) = belt_output_lane_index(segment, item.item_id()) else {
            return false;
        };
        let belt_item = BeltItem {
            id: transport.allocate_item_id(),
            item_id: item.item_id(),
            position_subtile: 0,
        };
        insert_lane_item_at_entry(&mut segment.lanes[lane_index], belt_item);
        transport.mark_items_changed(entity_id);
        transport.mark_active(TransportLaneKey::Belt {
            entity_id,
            lane_index,
        });
        return true;
    }

    let splitter_input_port = splitter_input_port_for_occupied_tile(entities, entity_id, drop_tile);
    if let Some(state) = entities.splitters.get_mut(&entity_id) {
        if item.count() != 1 {
            return false;
        }

        let Some(input_port) = splitter_input_port else {
            return false;
        };
        let Some(lane_index) = splitter_output_lane_index(state, input_port, item.item_id()) else {
            return false;
        };
        let belt_item = BeltItem {
            id: transport.allocate_item_id(),
            item_id: item.item_id(),
            position_subtile: 0,
        };
        insert_lane_item_at_entry(&mut state.input_lanes[input_port][lane_index], belt_item);
        transport.mark_items_changed(entity_id);
        transport.mark_active(TransportLaneKey::Splitter {
            entity_id,
            input_port,
            lane_index,
        });
        return true;
    }

    false
}

/// Whether an inserter may commit its held stack to a completed rocket.
///
/// Cargo is a single payload, not a normal one-slot stack. Keeping this rule in
/// the shared target predicate makes both the planning and commit checks reject
/// a second inserter in the same tick.
fn rocket_silo_cargo_target_accepts(
    catalog: &PrototypeCatalog,
    research: &ResearchState,
    entities: &EntityStore,
    entity_id: EntityId,
    item: ItemStack,
) -> bool {
    let Some(silo) = entities.rocket_silos.get(&entity_id) else {
        return false;
    };
    item.count() == 1
        && silo.rocket_ready()
        && matches!(silo.launch_phase, crate::machines::RocketLaunchPhase::Idle)
        && silo
            .cargo_inventory
            .slots()
            .first()
            .is_some_and(|slot| slot.is_empty())
        && item_slot_policy_accepts(
            catalog,
            research,
            entities,
            ItemSlotPolicy::RocketCargo,
            ItemSlotOperation::InserterInsert,
            item.item_id(),
        )
        && silo
            .cargo_inventory
            .can_insert(catalog, item.item_id(), item.count())
}
