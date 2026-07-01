use super::super::*;
use super::support::*;

#[test]
fn initial_simulation_reveals_player_chunk() {
    let sim = Simulation::new_test_world(123);
    let (player_x, player_y) = sim.player.tile_position();
    let coord = ChunkCoord {
        x: player_x.div_euclid(CHUNK_SIZE),
        y: player_y.div_euclid(CHUNK_SIZE),
    };

    assert!(sim.is_chunk_revealed(coord));
}

#[test]
fn moving_player_into_another_chunk_reveals_that_chunk() {
    let mut sim = Simulation::new_test_world(123);
    let target = ChunkCoord { x: 1, y: 0 };
    sim.player = PlayerState::centered_on_tile(target.x * CHUNK_SIZE, target.y * CHUNK_SIZE);

    sim.tick();

    assert!(sim.is_chunk_revealed(target));
}

#[test]
fn player_starts_on_walkable_generated_tile() {
    let sim = Simulation::new_test_world(123);
    let (x, y) = sim.player.tile_position();
    let tile = sim
        .world
        .tile_at(x, y)
        .expect("player start should be in a generated chunk");

    assert!(tile.collision.walkable);
    assert!(sim.can_player_occupy_tile(x, y));
}

#[test]
fn player_cannot_move_into_water() {
    let mut sim = Simulation::new_test_world(123);
    let (start, delta) = first_player_approach_to_water(&sim);
    let before = PlayerState::centered_on_tile(start.0, start.1);
    sim.player = before;

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_eq!(sim.player, before);
}

#[test]
fn player_generates_and_can_walk_into_streamed_walkable_chunk() {
    let mut sim = Simulation::new_test_world(123);
    let (start, delta, streamed_chunk) = first_player_approach_to_streamed_walkable_tile(&sim);
    let before = PlayerState::centered_on_tile(start.0, start.1);
    sim.player = before;

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_ne!(sim.player, before);
    assert!(sim.world.chunks.contains_key(&streamed_chunk));
}

#[test]
fn moving_or_ticking_far_from_origin_reveals_generated_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let player_chunk = ChunkCoord { x: 20, y: -17 };
    sim.player =
        PlayerState::centered_on_tile(player_chunk.x * CHUNK_SIZE, player_chunk.y * CHUNK_SIZE);

    sim.tick();

    for y in player_chunk.y - 1..=player_chunk.y + 1 {
        for x in player_chunk.x - 1..=player_chunk.x + 1 {
            let coord = ChunkCoord { x, y };
            assert!(sim.world.chunks.contains_key(&coord));
            assert!(sim.is_chunk_revealed(coord));
        }
    }
}

#[test]
fn player_cannot_move_into_occupied_entity_tile() {
    let mut sim = Simulation::new_test_world(123);
    let (start, delta) = first_player_approach_to_occupied_tile(&mut sim);
    let before = PlayerState::centered_on_tile(start.0, start.1);
    sim.player = before;

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_eq!(sim.player, before);
}

#[test]
fn player_axis_separated_movement_slides_along_blocked_edges() {
    let mut sim = Simulation::new_test_world(123);
    let (start, expected) = first_player_slide_fixture(&mut sim);
    sim.player = PlayerState::centered_on_tile(start.0, start.1);

    sim.move_player_by_tiles(1.0, 1.0);

    assert_eq!(sim.player.tile_position(), expected);
}
