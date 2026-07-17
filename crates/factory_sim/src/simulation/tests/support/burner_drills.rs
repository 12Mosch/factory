use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn place_burner_drill_on_resource(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, i64, i64, u32) {
    place_named_drill_on_resource(sim, "burner_mining_drill", resource_item)
}

pub(in crate::simulation::tests) fn place_named_drill_on_resource(
    sim: &mut Simulation,
    drill_name: &str,
    resource_item: ItemId,
) -> (EntityId, i64, i64, u32) {
    let drill = entity_id_by_name(&sim.world.prototypes, drill_name);
    for (x, y) in all_tile_coords(&sim.world) {
        let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
            continue;
        };
        if resource.resource_item != resource_item {
            continue;
        }
        if crate::placement::validate(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: drill,
                x,
                y,
                direction: Direction::North,
            },
        )
        .is_err()
        {
            continue;
        }

        let entity_id = crate::placement::place(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: drill,
                x,
                y,
                direction: Direction::North,
            },
        )
        .expect("validated drill target should be placeable");
        return (entity_id, x, y, resource.amount);
    }

    panic!("expected placeable resource tile for {drill_name}");
}

pub(in crate::simulation::tests) fn place_burner_drill_outputting_to_chest(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, EntityId, i64, i64, u32) {
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
            if crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: drill,
                    x,
                    y,
                    direction,
                },
            )
            .is_err()
            {
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
            if crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: chest,
                    x: output_x,
                    y: output_y,
                    direction: Direction::North,
                },
            )
            .is_err()
            {
                continue;
            }

            let drill_id = crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: drill,
                    x,
                    y,
                    direction,
                },
            )
            .expect("validated drill target should be placeable");
            let chest_id = crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: chest,
                    x: output_x,
                    y: output_y,
                    direction: Direction::North,
                },
            )
            .expect("validated chest output target should be placeable");
            return (drill_id, chest_id, x, y, resource.amount);
        }
    }

    panic!("expected burner drill fixture with adjacent chest output");
}

pub(in crate::simulation::tests) fn place_burner_drill_outputting_to_belt(
    sim: &mut Simulation,
    resource_item: ItemId,
) -> (EntityId, EntityId, i64, i64, u32) {
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
            if crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: drill,
                    x,
                    y,
                    direction,
                },
            )
            .is_err()
            {
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
            if crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: belt,
                    x: output_x,
                    y: output_y,
                    direction,
                },
            )
            .is_err()
            {
                continue;
            }

            let drill_id = crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: drill,
                    x,
                    y,
                    direction,
                },
            )
            .expect("validated drill target should be placeable");
            let belt_id = crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: belt,
                    x: output_x,
                    y: output_y,
                    direction,
                },
            )
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
    set_inventory_slot(&mut sim.player_inventory, 0, fuel_item, count);
    crate::entity_transfer::player_slot_to_mining_drill_fuel(sim, entity_id, 0)
        .expect("fuel should transfer to burner drill");
}
