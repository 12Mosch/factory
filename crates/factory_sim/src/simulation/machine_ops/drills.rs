use crate::simulation::*;

pub(in crate::simulation) fn first_resource_in_mining_area(
    world: &WorldSim,
    footprint: &EntityFootprint,
    mining_drill: &factory_data::MiningDrillPrototype,
) -> Option<(ManualMiningTarget, ItemId)> {
    let width = mining_drill.mining_area.x.min(footprint.width).max(0);
    let height = mining_drill.mining_area.y.min(footprint.height).max(0);

    for y in footprint.y..footprint.y + height {
        for x in footprint.x..footprint.x + width {
            let Some(resource) = world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            return Some((ManualMiningTarget { x, y }, resource.resource_item));
        }
    }

    None
}

pub(in crate::simulation) fn first_resource_in_mining_area_profiled<P: TickProfiler>(
    world: &WorldSim,
    footprint: &EntityFootprint,
    mining_drill: &factory_data::MiningDrillPrototype,
    profiler: &mut P,
) -> Option<(ManualMiningTarget, ItemId)> {
    let width = mining_drill.mining_area.x.min(footprint.width).max(0);
    let height = mining_drill.mining_area.y.min(footprint.height).max(0);

    for y in footprint.y..footprint.y + height {
        for x in footprint.x..footprint.x + width {
            let Some(resource) = world
                .tile_at_profiled(x, y, profiler)
                .and_then(|tile| tile.resource)
            else {
                continue;
            };
            return Some((ManualMiningTarget { x, y }, resource.resource_item));
        }
    }

    None
}

pub(in crate::simulation) fn drill_output_target(
    entities: &EntityStore,
    placed: &PlacedEntity,
) -> DrillOutputTarget {
    let (x, y) = drill_output_tile(placed);
    match entities.occupancy.entity_at(x, y) {
        None => DrillOutputTarget::InternalSlot,
        Some(entity_id) if entity_id == placed.id => DrillOutputTarget::InternalSlot,
        Some(entity_id) if entities.transport_belts.contains_key(&entity_id) => {
            DrillOutputTarget::Belt(entity_id)
        }
        Some(entity_id) if entities.splitters.contains_key(&entity_id) => {
            splitter_input_port_for_occupied_tile(entities, entity_id, (x, y)).map_or(
                DrillOutputTarget::Blocked,
                |input_port| DrillOutputTarget::Splitter {
                    entity_id,
                    input_port,
                },
            )
        }
        Some(entity_id) if entities.entity_inventories.contains_key(&entity_id) => {
            DrillOutputTarget::Inventory(entity_id)
        }
        Some(_) => DrillOutputTarget::Blocked,
    }
}

pub(in crate::simulation) fn drill_output_tile(placed: &PlacedEntity) -> (i32, i32) {
    match placed.direction {
        Direction::North => (
            placed.footprint.x + placed.footprint.width / 2,
            placed.footprint.y + placed.footprint.height,
        ),
        Direction::East => (
            placed.footprint.x + placed.footprint.width,
            placed.footprint.y + placed.footprint.height / 2,
        ),
        Direction::South => (
            placed.footprint.x + placed.footprint.width / 2,
            placed.footprint.y - 1,
        ),
        Direction::West => (
            placed.footprint.x - 1,
            placed.footprint.y + placed.footprint.height / 2,
        ),
    }
}

pub(in crate::simulation) fn drill_output_target_can_accept(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    output_target: DrillOutputTarget,
    internal_output_slot: Option<ItemStack>,
    item_id: ItemId,
    count: u16,
) -> bool {
    match output_target {
        DrillOutputTarget::InternalSlot => {
            output_slot_can_accept(catalog, internal_output_slot, item_id, count)
        }
        DrillOutputTarget::Inventory(entity_id) => entities
            .entity_inventories
            .get(&entity_id)
            .is_some_and(|inventory| inventory.can_insert(catalog, item_id, count)),
        DrillOutputTarget::Belt(entity_id) => entities
            .transport_belts
            .get(&entity_id)
            .is_some_and(|segment| belt_output_lane_index(segment, item_id).is_some()),
        DrillOutputTarget::Splitter {
            entity_id,
            input_port,
        } => entities
            .splitters
            .get(&entity_id)
            .is_some_and(|state| splitter_output_lane_index(state, input_port, item_id).is_some()),
        DrillOutputTarget::Blocked => false,
    }
}

pub(in crate::simulation) fn insert_drill_output(
    entities: &mut EntityStore,
    drill_entity_id: EntityId,
    output_target: DrillOutputTarget,
    item_id: ItemId,
    count: u16,
    catalog: &PrototypeCatalog,
) {
    match output_target {
        DrillOutputTarget::InternalSlot => {
            let state = entities
                .burner_drill_state_mut(drill_entity_id)
                .expect("burner drill id came from burner drill state map");
            insert_output_item(&mut state.output_slot, item_id, count);
        }
        DrillOutputTarget::Inventory(entity_id) => {
            entities
                .entity_inventories
                .get_mut(&entity_id)
                .expect("validated output inventory should still exist")
                .insert(catalog, item_id, count)
                .expect("validated output inventory should accept drill product");
        }
        DrillOutputTarget::Belt(entity_id) => {
            let segment = entities
                .transport_belts
                .get_mut(&entity_id)
                .expect("validated output belt should still exist");
            let lane_index = belt_output_lane_index(segment, item_id)
                .expect("validated belt lane should accept");
            segment.lanes[lane_index].items.insert(
                0,
                BeltItem {
                    item_id,
                    position_subtile: 0,
                },
            );
        }
        DrillOutputTarget::Splitter {
            entity_id,
            input_port,
        } => {
            let state = entities
                .splitters
                .get_mut(&entity_id)
                .expect("validated output splitter should still exist");
            let lane_index = splitter_output_lane_index(state, input_port, item_id)
                .expect("validated splitter lane should accept");
            state.input_lanes[input_port][lane_index].items.insert(
                0,
                BeltItem {
                    item_id,
                    position_subtile: 0,
                },
            );
        }
        DrillOutputTarget::Blocked => {
            unreachable!("blocked drill output is checked before mining")
        }
    }
}

pub(in crate::simulation) fn try_export_stored_drill_output(
    entities: &mut EntityStore,
    drill_entity_id: EntityId,
    output_target: DrillOutputTarget,
    catalog: &PrototypeCatalog,
) -> bool {
    if !matches!(
        output_target,
        DrillOutputTarget::Inventory(_)
            | DrillOutputTarget::Belt(_)
            | DrillOutputTarget::Splitter { .. }
    ) {
        return false;
    }

    let Some(stack) = entities
        .burner_drill_state(drill_entity_id)
        .ok()
        .and_then(|state| state.output_slot)
    else {
        return false;
    };

    if !drill_output_target_can_accept(catalog, entities, output_target, None, stack.item_id, 1) {
        return false;
    }

    insert_drill_output(
        entities,
        drill_entity_id,
        output_target,
        stack.item_id,
        1,
        catalog,
    );
    let state = entities
        .burner_drill_state_mut(drill_entity_id)
        .expect("burner drill id came from burner drill state map");
    remove_from_single_slot(&mut state.output_slot, stack.item_id, 1)
        .expect("stored drill output should still contain exported item");

    true
}
