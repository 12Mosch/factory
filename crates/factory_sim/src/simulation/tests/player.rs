use super::super::*;
use super::support::*;

#[test]
fn initial_simulation_reveals_player_chunk() {
    let sim = Simulation::new_test_world(123);
    let (player_x, player_y) = sim.player.tile_position();
    let coord = ChunkCoord::from_tile(player_x, player_y)
        .expect("initial player should be inside the chunk plane");

    assert!(sim.is_chunk_revealed(coord));
}

#[test]
fn moving_player_into_another_chunk_reveals_that_chunk() {
    let mut sim = Simulation::new_test_world(123);
    let target = ChunkCoord { x: 1, y: 0 };
    sim.player = PlayerState::centered_on_tile(target.min_tile().0, target.min_tile().1);

    sim.tick();

    assert!(sim.is_chunk_revealed(target));
}

#[test]
fn reveal_candidates_do_not_chart_ungenerated_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let missing = ChunkCoord { x: 20, y: -17 };
    sim.player = PlayerState::centered_on_tile(missing.x * CHUNK_SIZE, missing.y * CHUNK_SIZE);

    sim.reveal_generated_chunks_around_player(&[missing]);

    assert!(!sim.world.chunks.contains_key(&missing));
    assert!(!sim.is_chunk_revealed(missing));
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

    assert_eq!(sim.player, before);
    assert!(!sim.world.chunks.contains_key(&streamed_chunk));

    sim.tick();
    assert!(sim.world.chunks.contains_key(&streamed_chunk));

    sim.move_player_by_tiles(delta.0, delta.1);

    assert_ne!(sim.player, before);
}

#[test]
fn teleport_streams_and_reveals_at_most_one_chunk_per_tick() {
    let mut sim = Simulation::new_test_world(123);
    let player_chunk = ChunkCoord { x: 20, y: -17 };
    sim.player =
        PlayerState::centered_on_tile(player_chunk.x * CHUNK_SIZE, player_chunk.y * CHUNK_SIZE);
    let initial_chunk_count = sim.world.generated_chunk_count();

    sim.tick();

    assert_eq!(
        sim.world.generated_chunk_count(),
        initial_chunk_count + CHUNK_GENERATION_BUDGET_PER_TICK
    );
    assert!(sim.world.chunks.contains_key(&player_chunk));
    assert!(sim.is_chunk_revealed(player_chunk));

    for completed_ticks in 2..=9 {
        sim.tick();
        assert_eq!(
            sim.world.generated_chunk_count(),
            initial_chunk_count + completed_ticks
        );
    }

    for y in player_chunk.y - 1..=player_chunk.y + 1 {
        for x in player_chunk.x - 1..=player_chunk.x + 1 {
            let coord = ChunkCoord { x, y };
            assert!(sim.world.chunks.contains_key(&coord));
            assert!(sim.is_chunk_revealed(coord));
        }
    }
}

#[test]
fn generation_queue_uses_priority_then_stable_coordinate_order() {
    let mut sim = Simulation::new_test_world(123);
    let required_first = ChunkCoord { x: -9, y: 9 };
    let required_second = ChunkCoord { x: 9, y: 9 };
    let chart = ChunkCoord { x: -20, y: -20 };
    let prefetch = ChunkCoord { x: -30, y: -30 };

    sim.request_chunk_generation(prefetch, ChunkGenerationPriority::Prefetch);
    sim.request_chunk_generation(required_second, ChunkGenerationPriority::Required);
    sim.request_chunk_generation(chart, ChunkGenerationPriority::Chart);
    sim.request_chunk_generation(required_first, ChunkGenerationPriority::Required);

    for expected in [required_first, required_second, prefetch, chart] {
        assert_eq!(sim.process_chunk_generation_queue(1), 1);
        assert!(sim.world.chunks.contains_key(&expected));
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
