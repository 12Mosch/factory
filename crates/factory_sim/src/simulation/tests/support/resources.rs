use super::super::super::*;
use super::*;

pub(in crate::simulation::tests) fn first_resource_tile(
    world: &WorldSim,
) -> (i64, i64, ResourceCell) {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            // Only minable (solid) resources; fluid patches such as crude oil
            // are extracted by pumpjacks and cannot be hand-mined.
            if !tile.collision.minable {
                continue;
            }
            if let Some(resource) = tile.resource {
                let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                let (x, y) = chunk.coord.tile_at(local_x, local_y);
                return (x, y, resource);
            }
        }
    }

    panic!("expected at least one minable resource tile");
}

pub(in crate::simulation::tests) fn first_resource_tile_for_item(
    world: &WorldSim,
    resource_item: ItemId,
) -> (i64, i64, u32) {
    for chunk in world.chunks.values() {
        for (index, tile) in chunk.tiles.iter().enumerate() {
            let Some(resource) = tile.resource else {
                continue;
            };

            if resource.resource_item != resource_item {
                continue;
            }

            let (x, y) = tile_coord(chunk, index);
            return (x, y, resource.amount);
        }
    }

    panic!("expected at least one resource tile for {resource_item:?}");
}

pub(in crate::simulation::tests) fn first_placeable_resource_tile(
    sim: &Simulation,
    prototype_id: EntityPrototypeId,
    resource_item: ItemId,
) -> (i64, i64, u32) {
    for (x, y) in all_tile_coords(&sim.world) {
        let Some(resource) = sim.world.tile_at(x, y).and_then(|tile| tile.resource) else {
            continue;
        };
        if resource.resource_item == resource_item
            && crate::placement::validate(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .is_ok()
        {
            return (x, y, resource.amount);
        }
    }

    panic!("expected at least one placeable resource tile");
}

pub(in crate::simulation::tests) fn resource_amount_at(
    world: &WorldSim,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Option<u32> {
    world
        .tile_at(x, y)
        .and_then(|tile| tile.resource.map(|resource| resource.amount))
}

pub(in crate::simulation::tests) fn nearby_resource_pair(
    world: &WorldSim,
) -> (
    (WorldTileCoord, WorldTileCoord),
    (WorldTileCoord, WorldTileCoord),
) {
    let resources = all_tile_coords(world)
        .into_iter()
        .filter(|(x, y)| {
            // Only minable tiles: hand-mining tests must be able to mine both.
            world
                .tile_at(*x, *y)
                .is_some_and(|tile| tile.resource.is_some() && tile.collision.minable)
        })
        .collect::<Vec<_>>();

    for first in &resources {
        for second in &resources {
            if first == second {
                continue;
            }

            let dx = first.0 - second.0;
            let dy = first.1 - second.1;
            if dx * dx + dy * dy <= 6 {
                return (*first, *second);
            }
        }
    }

    panic!("expected two resource tiles close enough to mine from one position");
}

pub(in crate::simulation::tests) fn resource_tiles(
    world: &WorldSim,
) -> Vec<(WorldTileCoord, WorldTileCoord, ResourceCell)> {
    world
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .filter_map(move |(index, tile)| {
                    let resource = tile.resource?;
                    let (x, y) = tile_coord(chunk, index);
                    Some((x, y, resource))
                })
        })
        .collect()
}
