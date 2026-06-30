use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn place_burner_drill_on_resource(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, i32, i32, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    for (x, y) in all_tile_coords(&sim.world) {
        let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
            continue;
        };
        if resource.resource_item != resource_item {
            continue;
        }
        if sim.can_place_entity(drill, x, y, Direction::North).is_err() {
            continue;
        }

        let entity_id = sim
            .place_entity(drill, x, y, Direction::North)
            .expect("validated drill target should be placeable");
        return (entity_id, x, y, resource.amount);
    }

    panic!("expected placeable resource tile for burner drill");
}

pub(in crate::simulation::tests) fn place_burner_drill_outputting_to_chest(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, EntityId, i32, i32, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let chest = entity_id_by_name(&sim.world.prototypes, "chest");
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        for (x, y) in all_tile_coords(&sim.world) {
            let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }
            if sim.can_place_entity(drill, x, y, direction).is_err() {
                continue;
            }

            let footprint = sim
                .world
                .entity_footprint(drill, x, y, direction)
                .expect("validated drill prototype should have a footprint");
            let placed = PlacedEntity {
                id: EntityId::new(0),
                prototype_id: drill,
                x,
                y,
                direction,
                footprint,
            };
            let (output_x, output_y) = drill_output_tile(&placed);
            if sim
                .can_place_entity(chest, output_x, output_y, Direction::North)
                .is_err()
            {
                continue;
            }

            let drill_id = sim
                .place_entity(drill, x, y, direction)
                .expect("validated drill target should be placeable");
            let chest_id = sim
                .place_entity(chest, output_x, output_y, Direction::North)
                .expect("validated chest output target should be placeable");
            return (drill_id, chest_id, x, y, resource.amount);
        }
    }

    panic!("expected burner drill fixture with adjacent chest output");
}

pub(in crate::simulation::tests) fn place_burner_drill_outputting_to_belt(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, EntityId, i32, i32, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, "burner_mining_drill");
    let belt = entity_id_by_name(&sim.world.prototypes, "transport_belt");
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        for (x, y) in all_tile_coords(&sim.world) {
            let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
                continue;
            };
            if resource.resource_item != resource_item {
                continue;
            }
            if sim.can_place_entity(drill, x, y, direction).is_err() {
                continue;
            }

            let footprint = sim
                .world
                .entity_footprint(drill, x, y, direction)
                .expect("validated drill prototype should have a footprint");
            let placed = PlacedEntity {
                id: EntityId::new(0),
                prototype_id: drill,
                x,
                y,
                direction,
                footprint,
            };
            let (output_x, output_y) = drill_output_tile(&placed);
            if sim
                .can_place_entity(belt, output_x, output_y, direction)
                .is_err()
            {
                continue;
            }

            let drill_id = sim
                .place_entity(drill, x, y, direction)
                .expect("validated drill target should be placeable");
            let belt_id = sim
                .place_entity(belt, output_x, output_y, direction)
                .expect("validated belt output target should be placeable");
            return (drill_id, belt_id, x, y, resource.amount);
        }
    }

    panic!("expected burner drill fixture with adjacent belt output");
}

pub(in crate::simulation::tests) fn add_fuel_to_burner_drill(
    sim: &mut Simulation,
    entity_id: EntityId,
    fuel_item: ItemId,
    count: u16,
) {
    sim.player_inventory = Inventory::player();
    sim.player_inventory.slots[0] = Some(ItemStack {
        item_id: fuel_item,
        count,
    });
    sim.transfer_player_slot_to_burner_drill_fuel(entity_id, 0)
        .expect("fuel should transfer to burner drill");
}
