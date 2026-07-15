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

    let generated = world.ensure_chunk_generated(coord);
    assert_eq!(generated.generated_chunks(), &[coord]);
    assert_eq!(generated.revision(), 1);
    assert_eq!(world.chunk_revision(), 1);
    assert_eq!(world.generated_chunk_count(), 26);
    assert!(world.ensure_chunk_generated(coord).is_empty());
    assert_eq!(world.chunk_revision(), 1);
    assert_eq!(world.generated_chunk_count(), 26);
}

#[test]
fn generated_chunks_cache_terrain_pollution_absorption() {
    let mut world = WorldSim::new_seeded(123);
    let far_coord = ChunkCoord { x: 40, y: -37 };
    assert_eq!(
        world.ensure_chunk_generated(far_coord).generated_chunks(),
        &[far_coord]
    );

    for prototype in &world.prototypes.tiles {
        assert_eq!(
            world.generator.tile_pollution_absorption_per_minute_milli[prototype.id.index()],
            u64::from(prototype.pollution_absorption_per_minute_milli),
        );
    }

    for coord in [ChunkCoord { x: 0, y: 0 }, far_coord] {
        let chunk = &world.chunks[&coord];
        let expected: u64 = chunk
            .tiles
            .iter()
            .map(|tile| {
                u64::from(
                    world.prototypes.tiles[tile.tile_id.index()]
                        .pollution_absorption_per_minute_milli,
                )
            })
            .sum();
        assert_eq!(chunk.pollution_absorption_per_minute_milli, expected);
    }
}

#[test]
fn batch_generation_returns_only_new_coordinates_in_input_order() {
    let mut world = WorldSim::new_seeded(123);
    let existing = ChunkCoord { x: 0, y: 0 };
    let first = ChunkCoord { x: 40, y: -37 };
    let second = ChunkCoord { x: -41, y: 38 };

    let generated = world.ensure_chunks_generated([existing, first, first, second]);

    assert_eq!(generated.generated_chunks(), &[first, second]);
    assert_eq!(generated.revision(), 2);
    assert_eq!(world.chunk_revision(), 2);
    assert!(world.chunks.contains_key(&first));
    assert!(world.chunks.contains_key(&second));
}

#[test]
fn neighborhood_generation_uses_batch_missing_chunk_semantics() {
    let mut world = WorldSim::new_seeded(123);
    let center = ChunkCoord { x: 40, y: -37 };

    assert_eq!(
        world
            .ensure_chunks_around_chunk(center, 1)
            .map(|result| result.len()),
        Ok(9)
    );
    assert_eq!(world.chunk_revision(), 9);
    assert_eq!(
        world
            .ensure_chunks_around_chunk(center, 1)
            .map(|result| result.len()),
        Ok(0)
    );
    assert_eq!(world.chunk_revision(), 9);
}

#[test]
fn chunk_generation_history_returns_exact_coordinates_since_revision() {
    let mut world = WorldSim::new_seeded(123);
    let first = ChunkCoord { x: 40, y: -37 };
    let second = ChunkCoord { x: -41, y: 38 };

    world.ensure_chunk_generated(first);
    let after_first = world.chunk_revision();
    world.ensure_chunks_generated([first, second]);

    let generated = world
        .chunk_generation_since(after_first)
        .expect("recent generation should remain in history");
    assert_eq!(generated.generated_chunks(), &[second]);
    assert_eq!(generated.revision(), world.chunk_revision());

    // Prefill the runtime-only history, then use real generation to exercise
    // the world's production trimming path without generating thousands of
    // full terrain chunks in this unit test.
    for revision in 3..=10_000 {
        world.chunk_generation_history.0.push_back(
            crate::world::generation::ChunkGenerationChange {
                revision,
                coord: second,
            },
        );
    }
    world.chunk_revision = 10_000;
    world.ensure_chunk_generated(ChunkCoord {
        x: 10_000,
        y: 10_000,
    });

    let oldest_retained_revision = world
        .chunk_generation_history
        .0
        .front()
        .expect("generation history should retain its bounded tail")
        .revision
        - 1;
    assert!(
        world
            .chunk_generation_since(oldest_retained_revision)
            .is_some(),
        "the revision immediately before the oldest retained change should remain queryable"
    );
    assert!(
        world
            .chunk_generation_since(oldest_retained_revision - 1)
            .is_none(),
        "a revision older than the retained generation history should expire"
    );
}

#[test]
fn deserialization_rebuilds_the_runtime_world_generator() {
    let mut world = WorldSim::new_seeded(123);
    world
        .generator
        .tile_pollution_absorption_per_minute_milli
        .clear();
    let bytes = bincode::serialize(&world).expect("world should serialize");

    let loaded: WorldSim = bincode::deserialize(&bytes).expect("world should deserialize");

    assert_eq!(
        loaded
            .generator
            .tile_pollution_absorption_per_minute_milli
            .len(),
        loaded.prototypes.tiles.len()
    );
    for tile in &loaded.prototypes.tiles {
        assert_eq!(
            loaded.generator.tile_pollution_absorption_per_minute_milli[tile.id.index()],
            u64::from(tile.pollution_absorption_per_minute_milli)
        );
    }
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
