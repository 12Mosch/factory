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
fn reveal_history_returns_exact_new_chunks_since_revision() {
    let mut sim = Simulation::new_test_world(123);
    let target = ChunkCoord { x: 20, y: -17 };
    sim.world
        .ensure_chunks_around_chunk(target, 1)
        .expect("target reveal neighborhood should be representable");
    let before_revision = sim.revealed_revision();
    let before = sim.revealed_chunks().clone();
    sim.player = PlayerState::centered_on_tile(target.x * CHUNK_SIZE, target.y * CHUNK_SIZE);

    sim.request_chunks_around_player();

    let expected = sim
        .revealed_chunks()
        .difference(&before)
        .copied()
        .collect::<BTreeSet<_>>();
    let changed = sim
        .revealed_chunks_since(before_revision)
        .expect("recent reveal changes should remain available")
        .collect::<BTreeSet<_>>();
    assert!(!expected.is_empty());
    assert_eq!(changed, expected);
}

#[test]
fn missing_reveal_history_requests_a_rebuild() {
    let mut sim = Simulation::new_test_world(123);
    let previous_revision = sim.revealed_revision();
    sim.revealed_revision = previous_revision.wrapping_add(1);
    sim.revealed_chunk_history.0.clear();

    assert!(sim.revealed_chunks_since(previous_revision).is_none());
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
    let radar = ChunkCoord { x: 30, y: 30 };

    sim.request_chunk_generation(radar, ChunkGenerationPriority::RadarReveal);
    sim.request_chunk_generation(prefetch, ChunkGenerationPriority::Prefetch);
    sim.request_chunk_generation(required_second, ChunkGenerationPriority::Required);
    sim.request_chunk_generation(chart, ChunkGenerationPriority::PlayerChart);
    sim.request_chunk_generation(required_first, ChunkGenerationPriority::Required);

    for expected in [required_first, required_second, prefetch, chart, radar] {
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

fn first_player_tunnel_fixture(
    sim: &mut Simulation,
) -> (
    (WorldTileCoord, WorldTileCoord),
    (WorldTileCoord, WorldTileCoord),
) {
    let inserter = entity_id_by_name(&sim.world.prototypes, "inserter");

    for (x, y) in all_tile_coords(&sim.world) {
        let start = (x, y);
        let blocked = (x + 1, y);
        let destination = (x + 2, y);

        if crate::placement::validate(
            sim,
            crate::placement::EntityPlacementRequest {
                prototype_id: inserter,
                x: blocked.0,
                y: blocked.1,
                direction: Direction::North,
            },
        )
        .is_ok()
            && sim.can_player_occupy_tile(start.0, start.1)
            && sim.can_player_occupy_tile(destination.0, destination.1)
        {
            crate::placement::place(
                sim,
                crate::placement::EntityPlacementRequest {
                    prototype_id: inserter,
                    x: blocked.0,
                    y: blocked.1,
                    direction: Direction::North,
                },
            )
            .expect("validated tunnel blocker should be placeable");
            debug_assert!(!sim.can_player_occupy_tile(blocked.0, blocked.1));
            debug_assert!(sim.can_player_occupy_tile(destination.0, destination.1));
            return (start, destination);
        }
    }

    panic!("expected a walkable-blocked-walkable tile run for tunnel coverage");
}

fn first_player_open_run_fixture(
    sim: &Simulation,
) -> (
    (WorldTileCoord, WorldTileCoord),
    (WorldTileCoord, WorldTileCoord),
) {
    for (x, y) in all_tile_coords(&sim.world) {
        let start = (x, y);
        let middle = (x + 1, y);
        let destination = (x + 2, y);

        if sim.can_player_occupy_tile(start.0, start.1)
            && sim.can_player_occupy_tile(middle.0, middle.1)
            && sim.can_player_occupy_tile(destination.0, destination.1)
        {
            return (start, destination);
        }
    }

    panic!("expected an open three-tile run for long movement coverage");
}

#[test]
fn player_long_step_cannot_tunnel_through_blocked_tile() {
    let mut sim = Simulation::new_test_world(123);
    let (start, destination) = first_player_tunnel_fixture(&mut sim);
    sim.player = PlayerState::centered_on_tile(start.0, start.1);

    sim.move_player_by_tiles(2.0, 0.0);

    assert_eq!(
        sim.player.tile_position(),
        start,
        "a two-tile step must stop before the blocked intermediate tile instead of landing on {destination:?}"
    );
}

#[test]
fn player_large_frame_delta_cannot_tunnel_through_blocked_tile() {
    let mut sim = Simulation::new_test_world(123);
    let (start, _destination) = first_player_tunnel_fixture(&mut sim);
    sim.player = PlayerState::centered_on_tile(start.0, start.1);

    sim.move_player(1.0, 0.0, 1.0);

    assert_eq!(
        sim.player.tile_position(),
        start,
        "a large frame delta must not carry the player through the blocked tile"
    );
}

#[test]
fn player_long_step_succeeds_without_obstruction() {
    let mut sim = Simulation::new_test_world(123);
    let (start, destination) = first_player_open_run_fixture(&sim);
    sim.player = PlayerState::centered_on_tile(start.0, start.1);

    sim.move_player_by_tiles(2.0, 0.0);

    assert_eq!(
        sim.player.tile_position(),
        destination,
        "unobstructed multi-tile movement must still reach the destination"
    );
}
