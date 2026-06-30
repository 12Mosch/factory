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

    let generated_side = (WORLD_MAX_CHUNK - WORLD_MIN_CHUNK + 1) as usize;
    assert_eq!(world.chunks.len(), generated_side * generated_side);
    for chunk in world.chunks.values() {
        assert_eq!(chunk.tiles.len(), (CHUNK_SIZE * CHUNK_SIZE) as usize);
    }
}

#[test]
fn resource_generation_is_deterministic() {
    let a = WorldSim::new_seeded(123);
    let b = WorldSim::new_seeded(123);

    assert_eq!(resource_tiles(&a), resource_tiles(&b));
}

#[test]
fn seed_123_contains_all_resource_item_types() {
    let world = WorldSim::new_seeded(123);
    let ids = WorldPrototypeIds::from_catalog(&world.prototypes);
    let resource_items = world
        .chunks
        .values()
        .flat_map(|chunk| chunk.tiles.iter())
        .filter_map(|tile| tile.resource.map(|resource| resource.resource_item))
        .collect::<BTreeSet<_>>();

    for resource_item in ids.resources {
        assert!(
            resource_items.contains(&resource_item),
            "missing generated resource item {resource_item:?}"
        );
    }
}
