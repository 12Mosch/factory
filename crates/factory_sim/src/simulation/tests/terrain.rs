use super::super::*;
use super::support::*;

use factory_data::BasePrototypeIds;

fn base_ids(sim: &Simulation) -> BasePrototypeIds {
    BasePrototypeIds::from_catalog(sim.catalog())
}

/// A generated, resource-free ground tile with no entity on it. Terrain tests
/// build their own water from this so they never depend on a seed happening to
/// generate a lake inside the starting chunks.
fn plain_ground_tile(sim: &Simulation) -> (WorldTileCoord, WorldTileCoord) {
    let (player_x, player_y) = sim.player.tile_position();
    all_tile_coords(&sim.world)
        .into_iter()
        .find(|&(x, y)| {
            (x, y) != (player_x, player_y)
                && sim.entities.occupancy.entity_at(x, y).is_none()
                && sim.world.tile_at(x, y).is_some_and(|tile| {
                    tile.resource.is_none() && tile.collision.walkable && tile.collision.buildable
                })
        })
        .expect("test world should contain a plain ground tile")
}

fn flooded_tile(sim: &mut Simulation) -> (WorldTileCoord, WorldTileCoord) {
    let water = base_ids(sim).tiles.water;
    let (x, y) = plain_ground_tile(sim);
    sim.world
        .set_tile(x, y, water)
        .expect("ground tile should accept a water rewrite");
    (x, y)
}

fn give_player(sim: &mut Simulation, item_id: ItemId, count: u16) {
    sim.player_inventory
        .insert(&sim.world.prototypes, item_id, count)
        .expect("test player inventory should accept the placement items");
}

fn place_tile(
    sim: &mut Simulation,
    item_id: ItemId,
    x: WorldTileCoord,
    y: WorldTileCoord,
) -> Result<SimCommandEffect, SimCommandError> {
    sim.apply_command(&SimCommand::PlaceTileFromPlayerInventory { item_id, x, y })
}

#[test]
fn landfill_turns_water_into_buildable_ground() {
    let mut sim = Simulation::new_test_world(4242);
    let ids = base_ids(&sim);
    let (x, y) = flooded_tile(&mut sim);
    give_player(&mut sim, ids.items.landfill, 1);
    assert!(
        !sim.world
            .tile_at(x, y)
            .expect("flooded tile")
            .collision
            .walkable
    );

    place_tile(&mut sim, ids.items.landfill, x, y).expect("landfill should fill water");

    let tile = sim.world.tile_at(x, y).expect("filled tile should exist");
    assert_eq!(tile.tile_id, ids.tiles.landfill);
    assert!(tile.collision.walkable);
    assert!(tile.collision.buildable);
    assert_eq!(sim.player_inventory.count(ids.items.landfill), 0);
}

#[test]
fn landfill_is_rejected_on_solid_ground() {
    let mut sim = Simulation::new_test_world(4243);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    give_player(&mut sim, ids.items.landfill, 1);
    let before = sim.world.tile_at(x, y).expect("ground tile").tile_id;

    let error = place_tile(&mut sim, ids.items.landfill, x, y)
        .expect_err("landfill must not be usable on dry land");

    assert_eq!(
        error,
        SimCommandError::TilePlacement(TilePlacementError::RequiresWater { x, y })
    );
    assert_eq!(
        sim.world.tile_at(x, y).expect("ground tile").tile_id,
        before
    );
    assert_eq!(sim.player_inventory.count(ids.items.landfill), 1);
}

#[test]
fn paving_is_rejected_on_water() {
    let mut sim = Simulation::new_test_world(4244);
    let ids = base_ids(&sim);
    let (x, y) = flooded_tile(&mut sim);
    give_player(&mut sim, ids.items.stone_brick, 1);

    let error = place_tile(&mut sim, ids.items.stone_brick, x, y)
        .expect_err("paving must not bridge water");

    assert_eq!(
        error,
        SimCommandError::TilePlacement(TilePlacementError::RequiresSolidGround { x, y })
    );
    assert_eq!(sim.player_inventory.count(ids.items.stone_brick), 1);
}

#[test]
fn stone_brick_paves_a_stone_path() {
    let mut sim = Simulation::new_test_world(4245);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    give_player(&mut sim, ids.items.stone_brick, 2);

    place_tile(&mut sim, ids.items.stone_brick, x, y).expect("stone brick should pave ground");

    assert_eq!(
        sim.world.tile_at(x, y).expect("paved tile").tile_id,
        ids.tiles.stone_path
    );
    assert_eq!(sim.player_inventory.count(ids.items.stone_brick), 1);
}

#[test]
fn repaving_the_same_tile_is_rejected_without_consuming_an_item() {
    let mut sim = Simulation::new_test_world(4246);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    give_player(&mut sim, ids.items.concrete, 2);
    place_tile(&mut sim, ids.items.concrete, x, y).expect("concrete should pave ground");

    let error = place_tile(&mut sim, ids.items.concrete, x, y)
        .expect_err("repaving an identical tile must be rejected");

    assert_eq!(
        error,
        SimCommandError::TilePlacement(TilePlacementError::AlreadyPlaced { x, y })
    );
    assert_eq!(sim.player_inventory.count(ids.items.concrete), 1);
}

#[test]
fn paving_without_the_item_leaves_the_world_untouched() {
    let mut sim = Simulation::new_test_world(4247);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    let before = sim.world.terrain_revision();

    let error = place_tile(&mut sim, ids.items.concrete, x, y)
        .expect_err("paving without the item must be rejected");

    assert_eq!(
        error,
        SimCommandError::TilePlacement(TilePlacementError::InsufficientInventory {
            item_id: ids.items.concrete,
        })
    );
    assert_eq!(sim.world.terrain_revision(), before);
}

#[test]
fn paved_tiles_speed_the_player_up() {
    let mut sim = Simulation::new_test_world(4248);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    sim.player = PlayerState::centered_on_tile(x, y);
    assert_eq!(sim.player_walking_speed_multiplier(), 1.0);

    give_player(&mut sim, ids.items.concrete, 1);
    place_tile(&mut sim, ids.items.concrete, x, y).expect("concrete should pave ground");

    let expected = f32::from(
        sim.catalog()
            .tile(ids.tiles.concrete)
            .expect("concrete tile prototype")
            .walking_speed_percent,
    ) / 100.0;
    assert_eq!(sim.player_walking_speed_multiplier(), expected);
    assert!(expected > 1.0, "concrete should be faster than bare ground");
}

#[test]
fn terrain_history_reports_exact_tiles_since_a_revision() {
    let mut sim = Simulation::new_test_world(4249);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    give_player(&mut sim, ids.items.stone_brick, 1);
    let before = sim.world.terrain_revision();

    place_tile(&mut sim, ids.items.stone_brick, x, y).expect("stone brick should pave ground");

    let changes = sim
        .world
        .terrain_dirty_tiles_since(before)
        .expect("recent terrain changes should remain available")
        .collect::<Vec<_>>();
    assert_eq!(changes.len(), 1);
    assert_eq!((changes[0].x, changes[0].y), (x, y));
    assert_eq!(changes[0].tile_id, ids.tiles.stone_path);
    assert!(sim.world.terrain_revision() > before);
}

#[test]
fn terrain_history_reports_falling_behind() {
    let mut sim = Simulation::new_test_world(4250);
    let ids = base_ids(&sim);
    let stale = sim.world.terrain_revision();
    let tiles = all_tile_coords(&sim.world);
    let mut written = 0;

    // Overrun the bounded history so the caller must rebuild from `chunks`.
    for (x, y) in tiles {
        if written > WorldSim::TERRAIN_DIRTY_HISTORY_LIMIT {
            break;
        }
        if sim.world.set_tile(x, y, ids.tiles.landfill).is_ok() {
            written += 1;
        }
    }

    assert!(written > WorldSim::TERRAIN_DIRTY_HISTORY_LIMIT);
    assert!(sim.world.terrain_dirty_tiles_since(stale).is_none());
    assert!(
        sim.world
            .terrain_dirty_tiles_since(sim.world.terrain_revision())
            .is_some()
    );
}

#[test]
fn saved_worlds_round_trip_mutated_terrain() {
    let mut sim = Simulation::new_test_world(4255);
    let ids = base_ids(&sim);
    let (x, y) = flooded_tile(&mut sim);
    give_player(&mut sim, ids.items.landfill, 1);
    place_tile(&mut sim, ids.items.landfill, x, y).expect("landfill should fill water");
    let coord = ChunkCoord::from_tile(x, y).expect("tile should be inside the chunk plane");
    let absorption = sim.world.chunks[&coord].pollution_absorption_per_minute_milli;

    let bytes = save_to_bytes(&sim).expect("mutated terrain should save");
    let loaded = load_from_bytes(&bytes).expect("mutated terrain should load");

    let tile = loaded
        .world()
        .tile_at(x, y)
        .expect("filled tile should load");
    assert_eq!(tile.tile_id, ids.tiles.landfill);
    assert!(tile.collision.walkable);
    assert!(tile.collision.buildable);
    assert_eq!(
        loaded.world().chunks[&coord].pollution_absorption_per_minute_milli,
        absorption,
        "the rebuilt absorption cache should match the mutated terrain"
    );
    assert_eq!(sim.state_hash(), loaded.state_hash());
}

#[test]
fn terrain_writes_keep_the_chunk_pollution_cache_exact() {
    let mut sim = Simulation::new_test_world(4251);
    let ids = base_ids(&sim);
    let (x, y) = plain_ground_tile(&sim);
    let coord = ChunkCoord::from_tile(x, y).expect("tile should be inside the chunk plane");

    sim.world
        .set_tile(x, y, ids.tiles.concrete)
        .expect("ground tile should accept concrete");

    let cached = sim.world.chunks[&coord].pollution_absorption_per_minute_milli;
    let recomputed: u64 = sim.world.chunks[&coord]
        .tiles
        .iter()
        .map(|tile| {
            u64::from(
                sim.catalog()
                    .tile(tile.tile_id)
                    .expect("chunk tile should have a prototype")
                    .pollution_absorption_per_minute_milli,
            )
        })
        .sum();
    assert_eq!(cached, recomputed);
}

#[test]
fn terrain_writes_preserve_resource_cells() {
    let mut sim = Simulation::new_test_world(4252);
    let ids = base_ids(&sim);
    let (x, y, resource) = first_resource_tile(&sim.world);

    sim.world
        .set_tile(x, y, ids.tiles.concrete)
        .expect("resource tile should accept concrete");

    let tile = sim.world.tile_at(x, y).expect("paved resource tile");
    assert_eq!(tile.tile_id, ids.tiles.concrete);
    assert_eq!(tile.resource, Some(resource));
    // Resource collision still wins over the paved terrain's own rules.
    assert!(tile.collision.walkable);
    assert!(!tile.collision.buildable);
    assert!(tile.collision.minable);
}

#[test]
fn setting_an_unchanged_tile_is_rejected() {
    let mut sim = Simulation::new_test_world(4253);
    let (x, y) = plain_ground_tile(&sim);
    let tile_id = sim.world.tile_at(x, y).expect("ground tile").tile_id;

    assert_eq!(
        sim.world.set_tile(x, y, tile_id),
        Err(TerrainMutationError::Unchanged { x, y })
    );
}

#[test]
fn setting_a_tile_outside_generated_chunks_is_rejected() {
    let mut sim = Simulation::new_test_world(4254);
    let ids = base_ids(&sim);
    let (x, y) = (5_000_000, 5_000_000);

    assert_eq!(
        sim.world.set_tile(x, y, ids.tiles.landfill),
        Err(TerrainMutationError::OutsideGeneratedChunks { x, y })
    );
}
