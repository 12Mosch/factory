use crate::simulation::*;

pub(in crate::simulation) fn first_resource_in_mining_area(
    world: &WorldSim,
    footprint: &EntityFootprint,
    mining_drill: &factory_data::MiningDrillPrototype,
) -> Option<(ManualMiningTarget, ItemId)> {
    for (x, y) in mining_area_tiles(footprint, mining_drill) {
        let Some(resource) = world.tile_at(x, y).and_then(|tile| tile.resource) else {
            continue;
        };
        if is_fluid_resource_item(&world.prototypes, resource.resource_item) {
            continue;
        }
        return Some((ManualMiningTarget { x, y }, resource.resource_item));
    }

    None
}

pub(in crate::simulation) fn first_resource_in_mining_area_profiled<P: TickProfiler>(
    world: &WorldSim,
    footprint: &EntityFootprint,
    mining_drill: &factory_data::MiningDrillPrototype,
    profiler: &mut P,
) -> Option<(ManualMiningTarget, ItemId)> {
    for (x, y) in mining_area_tiles(footprint, mining_drill) {
        let Some(resource) = world
            .tile_at_profiled(x, y, profiler)
            .and_then(|tile| tile.resource)
        else {
            continue;
        };
        if is_fluid_resource_item(&world.prototypes, resource.resource_item) {
            continue;
        }
        return Some((ManualMiningTarget { x, y }, resource.resource_item));
    }

    None
}

pub(in crate::simulation) fn mining_area_tiles(
    footprint: &EntityFootprint,
    mining_drill: &factory_data::MiningDrillPrototype,
) -> Vec<(i32, i32)> {
    let width = mining_drill.mining_area.x.min(footprint.width).max(0);
    let height = mining_drill.mining_area.y.min(footprint.height).max(0);
    let mut tiles = Vec::with_capacity((width * height) as usize);

    for y in footprint.y..footprint.y + height {
        for x in footprint.x..footprint.x + width {
            tiles.push((x, y));
        }
    }

    tiles
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
