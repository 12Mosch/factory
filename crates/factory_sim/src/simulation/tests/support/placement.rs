use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn first_manual_mining_reach_tile(
    sim: &Simulation,
    target_x: WorldTileCoord,
    target_y: WorldTileCoord,
) -> (WorldTileCoord, WorldTileCoord) {
    for dy in -2..=2 {
        for dx in -2..=2 {
            if dx * dx + dy * dy > 4 {
                continue;
            }

            let x = target_x + i64::from(dx);
            let y = target_y + i64::from(dy);
            if sim.can_player_occupy_tile(x, y) {
                return (x, y);
            }
        }
    }

    panic!("expected a reachable walkable tile near manual mining target");
}

pub(in crate::simulation::tests) fn fill_inventory_with(
    sim: &mut Simulation,
    entity_id: EntityId,
    item_id: ItemId,
) {
    let catalog = sim.world.prototypes.clone();
    let stack_size =
        item_stack_size(&catalog, item_id).expect("test item should have a stack size");
    let inventory = crate::entity_access::inventory_mut(sim, entity_id)
        .expect("test entity should have inventory");
    let stack = ItemStack::new(&catalog, item_id, stack_size)
        .expect("test item should form a full valid stack");
    *inventory = Inventory::from_slots(&catalog, vec![test_slot(stack); inventory.slots().len()])
        .expect("filled test inventory should be valid");
}

pub(in crate::simulation::tests) fn first_buildable_rect_without_resource(
    world: &WorldSim,
    width: i32,
    height: i32,
) -> (WorldTileCoord, WorldTileCoord) {
    for chunk in world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let (x, y) = tile_coord(chunk, index);
            let footprint = EntityFootprint {
                x,
                y,
                width,
                height,
            };

            if world.validate_entity_footprint(&footprint).is_ok()
                && footprint.tiles().iter().all(|(tile_x, tile_y)| {
                    world
                        .tile_at(*tile_x, *tile_y)
                        .and_then(|tile| tile.resource)
                        .is_none()
                })
            {
                return (x, y);
            }
        }
    }

    panic!("expected buildable area without resources");
}

pub(in crate::simulation::tests) fn first_water_tile(
    world: &WorldSim,
) -> (WorldTileCoord, WorldTileCoord) {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if !tile.collision.buildable {
                return tile_coord(chunk, index);
            }
        }
    }

    panic!("expected at least one water tile");
}

pub(in crate::simulation::tests) fn first_buildable_rect(
    world: &WorldSim,
    width: i32,
    height: i32,
) -> (WorldTileCoord, WorldTileCoord) {
    for chunk in world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let (x, y) = tile_coord(chunk, index);
            let footprint = EntityFootprint {
                x,
                y,
                width,
                height,
            };

            if world.validate_entity_footprint(&footprint).is_ok() {
                return (x, y);
            }
        }
    }

    panic!("expected at least one buildable {width}x{height} area");
}

pub(in crate::simulation::tests) fn first_player_approach_to_water(
    sim: &Simulation,
) -> ((WorldTileCoord, WorldTileCoord), (f32, f32)) {
    for chunk in sim.world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            if tile.collision.walkable {
                continue;
            }

            let (x, y) = tile_coord(chunk, index);
            for (dx, dy) in CARDINAL_DIRECTIONS {
                let start = (x - dx, y - dy);
                if sim.can_player_occupy_tile(start.0, start.1) {
                    return (start, (dx as f32, dy as f32));
                }
            }
        }
    }

    panic!("expected a water tile with a walkable adjacent approach");
}

pub(in crate::simulation::tests) fn first_player_approach_to_streamed_walkable_tile(
    sim: &Simulation,
) -> ((WorldTileCoord, WorldTileCoord), (f32, f32), ChunkCoord) {
    for chunk in sim.world.chunks.values() {
        for (index, _) in chunk.tiles.iter().enumerate() {
            let (x, y) = tile_coord(chunk, index);
            if !sim.can_player_occupy_tile(x, y) {
                continue;
            }

            for (dx, dy) in CARDINAL_DIRECTIONS {
                let target_x = x + dx;
                let target_y = y + dy;
                if sim.world.tile_at(target_x, target_y).is_some() {
                    continue;
                }

                let target_chunk = ChunkCoord::from_tile(target_x, target_y)
                    .expect("streamed target should remain in the chunk plane");
                let mut world = sim.world.clone();
                world.ensure_chunk_generated(target_chunk);
                if world
                    .tile_at(target_x, target_y)
                    .is_some_and(|tile| tile.collision.walkable)
                {
                    return ((x, y), (dx as f32, dy as f32), target_chunk);
                }
            }
        }
    }

    panic!("expected a walkable boundary tile next to a streamable walkable chunk");
}

pub(in crate::simulation::tests) fn first_player_approach_to_occupied_tile(
    sim: &mut Simulation,
) -> ((WorldTileCoord, WorldTileCoord), (f32, f32)) {
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

    for (x, y) in all_tile_coords(&sim.world) {
        if crate::placement::validate(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: inserter,
                x,
                y,
                direction: Direction::North,
            },
        )
        .is_err()
        {
            continue;
        }

        for (dx, dy) in CARDINAL_DIRECTIONS {
            let start = (x - dx, y - dy);
            if sim.can_player_occupy_tile(start.0, start.1) {
                crate::placement::place(
                    sim,
                    crate::placement::EntityPlacementRequest {
                        prototype_id: inserter,
                        x,
                        y,
                        direction: Direction::North,
                    },
                )
                .expect("validated occupied target should be placeable");
                return (start, (dx as f32, dy as f32));
            }
        }
    }

    panic!("expected a placeable entity tile with a walkable adjacent approach");
}

pub(in crate::simulation::tests) fn first_player_slide_fixture(
    sim: &mut Simulation,
) -> (
    (WorldTileCoord, WorldTileCoord),
    (WorldTileCoord, WorldTileCoord),
) {
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

    for (x, y) in all_tile_coords(&sim.world) {
        let start = (x - 1, y);
        let expected = (x - 1, y + 1);

        if crate::placement::validate(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: inserter,
                x,
                y,
                direction: Direction::North,
            },
        )
        .is_ok()
            && sim.can_player_occupy_tile(start.0, start.1)
            && sim.can_player_occupy_tile(expected.0, expected.1)
        {
            crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: inserter,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .expect("validated slide blocker should be placeable");
            return (start, expected);
        }
    }

    panic!("expected a slide fixture with an occupied x-axis target and open y-axis target");
}

pub(in crate::simulation::tests) fn tile_coord(
    chunk: &Chunk,
    index: usize,
) -> (WorldTileCoord, WorldTileCoord) {
    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
    chunk.coord.tile_at(local_x, local_y)
}

pub(in crate::simulation::tests) fn all_tile_coords(
    world: &WorldSim,
) -> Vec<(WorldTileCoord, WorldTileCoord)> {
    world
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .map(move |(index, _)| tile_coord(chunk, index))
        })
        .collect()
}

pub(in crate::simulation::tests) fn first_placeable_entity_tile(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    direction: Direction,
) -> (WorldTileCoord, WorldTileCoord) {
    for (x, y) in all_tile_coords(&sim.world) {
        if crate::placement::validate(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id,
                x,
                y,
                direction,
            },
        )
        .is_ok()
        {
            return (x, y);
        }
    }

    panic!("expected at least one placeable entity tile");
}

pub(in crate::simulation::tests) fn place_at(
    sim: &mut Simulation,
    prototype_id: EntityPrototypeId,
    x: WorldTileCoord,
    y: WorldTileCoord,
    direction: Direction,
) -> EntityId {
    crate::placement::place(
        sim,
        crate::placement::EntityPlacementRequest {
            prototype_id,
            x,
            y,
            direction,
        },
    )
    .expect("test entity should be placeable")
}

pub(in crate::simulation::tests) const CARDINAL_DIRECTIONS: [(WorldTileCoord, WorldTileCoord); 4] =
    [(1, 0), (-1, 0), (0, 1), (0, -1)];
