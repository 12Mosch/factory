use super::super::*;
use super::support::*;
use std::collections::BTreeSet;

#[test]
fn world_tile_lookup_is_stable_across_chunk_boundaries() {
    let world = WorldSim::new_seeded(123);

    let left_of_origin = world.tile_at(-1, 0).expect("-1 should be in chunk -1");
    let previous_chunk_tile = world.tile_at(-33, 0).expect("-33 should be in chunk -2");
    let previous_chunk = world
        .chunks
        .get(&ChunkCoord { x: -2, y: 0 })
        .expect("previous negative chunk should exist");

    assert_eq!(
        left_of_origin,
        &world
            .chunks
            .get(&ChunkCoord { x: -1, y: 0 })
            .expect("left chunk should exist")
            .tiles[31]
    );
    assert!(world.tile_at(-32, 0).is_some());
    assert_eq!(previous_chunk_tile, &previous_chunk.tiles[31]);
}

#[test]
fn generated_chunks_have_expected_shape() {
    let world = WorldSim::new_seeded(123);

    let area = world.prototypes.world_generation.starting_area;
    let generated_side = (area.max_chunk - area.min_chunk + 1) as usize;
    assert_eq!(world.chunks.len(), generated_side * generated_side);
    for chunk in world.chunks.values() {
        assert_eq!(chunk.tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
    }
}

#[test]
fn initial_world_still_generates_twenty_five_chunks() {
    let world = WorldSim::new_seeded(123);

    assert_eq!(world.generated_chunk_count(), 25);
}

#[test]
fn ensure_chunk_generated_creates_missing_chunk_once() {
    let mut world = WorldSim::new_seeded(123);
    let coord = ChunkCoord { x: 40, y: -37 };

    assert!(world.ensure_chunk_generated(coord));
    assert_eq!(world.chunk_revision(), 1);
    assert_eq!(world.generated_chunk_count(), 26);
    assert!(!world.ensure_chunk_generated(coord));
    assert_eq!(world.chunk_revision(), 1);
    assert_eq!(world.generated_chunk_count(), 26);
}

#[test]
fn chunk_generation_is_independent_of_generation_order() {
    let coord_a = ChunkCoord { x: 12, y: -9 };
    let coord_b = ChunkCoord { x: -11, y: 14 };
    let mut first = WorldSim::new_seeded(123);
    let mut second = WorldSim::new_seeded(123);

    first.ensure_chunk_generated(coord_a);
    first.ensure_chunk_generated(coord_b);
    second.ensure_chunk_generated(coord_b);
    second.ensure_chunk_generated(coord_a);

    assert_eq!(first.chunks.get(&coord_a), second.chunks.get(&coord_a));
    assert_eq!(first.chunks.get(&coord_b), second.chunks.get(&coord_b));
}

#[test]
fn far_chunk_resources_are_deterministic_across_sims() {
    let coord = ChunkCoord { x: 50, y: -44 };
    let mut first = WorldSim::new_seeded(9876);
    let mut second = WorldSim::new_seeded(9876);

    first.ensure_chunk_generated(coord);
    second.ensure_chunk_generated(coord);

    let first_resources = resource_tiles_in_chunk(&first, coord);
    let second_resources = resource_tiles_in_chunk(&second, coord);
    assert_eq!(first_resources, second_resources);
}

#[test]
fn resource_generation_is_deterministic() {
    let a = WorldSim::new_seeded(123);
    let b = WorldSim::new_seeded(123);

    assert_eq!(resource_tiles(&a), resource_tiles(&b));
}

fn resource_tiles_in_chunk(
    world: &WorldSim,
    coord: ChunkCoord,
) -> Vec<(WorldTileCoord, WorldTileCoord, ResourceCell)> {
    let chunk = world
        .chunks
        .get(&coord)
        .expect("test chunk should be generated");
    chunk
        .tiles
        .iter()
        .enumerate()
        .filter_map(|(index, tile)| {
            let resource = tile.resource?;
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let (x, y) = coord.tile_at(local_x, local_y);
            Some((x, y, resource))
        })
        .collect()
}

#[test]
fn seed_123_contains_all_resource_item_types() {
    let world = WorldSim::new_seeded(123);
    let resource_items = world
        .chunks
        .values()
        .flat_map(|chunk| chunk.tiles.iter())
        .filter_map(|tile| tile.resource.map(|resource| resource.resource_item))
        .collect::<BTreeSet<_>>();

    let configured = &world.prototypes.world_generation.resources;
    assert!(!configured.is_empty());
    for resource in configured {
        assert!(
            resource_items.contains(&resource.resource_item),
            "missing generated resource item {:?}",
            resource.resource_item
        );
    }
}
