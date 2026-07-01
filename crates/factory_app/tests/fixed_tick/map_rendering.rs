use super::common::pixel_at;
use factory_app::rendering::map_texture::{
    GRID_PIXEL, PLAYER_PIXEL, UNREVEALED_PIXEL, generate_map_pixels,
};
use factory_app::resources::MapDisplaySettings;
use factory_sim::{CHUNK_SIZE, ChunkCoord, Simulation, WorldSim};

#[test]
fn map_pixel_generation_draws_reveal_player_and_debug_grid() {
    let sim = Simulation::new_test_world(123);
    let normal = generate_map_pixels(&sim, &MapDisplaySettings::default());
    let player_tile = sim.player().tile_position();

    assert_eq!(pixel_at(&normal, player_tile), PLAYER_PIXEL);

    let unrevealed_chunk = sim
        .world()
        .chunks
        .keys()
        .copied()
        .find(|coord| !sim.is_chunk_revealed(*coord))
        .expect("initial chart should leave distant chunks unrevealed");
    assert_eq!(
        pixel_at(
            &normal,
            (
                unrevealed_chunk.x * CHUNK_SIZE + 1,
                unrevealed_chunk.y * CHUNK_SIZE + 1
            )
        ),
        UNREVEALED_PIXEL
    );

    let debug = generate_map_pixels(
        &sim,
        &MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: true,
        },
    );
    assert_eq!(pixel_at(&debug, (0, 0)), GRID_PIXEL);
}

#[test]
fn map_pixels_show_streamed_revealed_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let target_chunk = ChunkCoord { x: 0, y: -9 };
    let target = first_walkable_tile_in_chunk(sim.seed(), target_chunk);
    move_player_to_tile(&mut sim, target);
    sim.tick();

    let map = generate_map_pixels(&sim, &MapDisplaySettings::default());

    assert_ne!(pixel_at(&map, target), UNREVEALED_PIXEL);
}

#[test]
fn generated_unrevealed_streamed_chunks_remain_hidden_until_revealed() {
    let mut sim = Simulation::new_test_world(123);
    let target_chunk = ChunkCoord { x: 0, y: 11 };
    let target = first_walkable_tile_in_chunk(sim.seed(), target_chunk);
    move_player_to_tile(&mut sim, target);
    let sample = (
        target_chunk.x * CHUNK_SIZE + (target.0 + 1).rem_euclid(CHUNK_SIZE),
        target.1,
    );

    let map = generate_map_pixels(&sim, &MapDisplaySettings::default());

    assert_eq!(pixel_at(&map, sample), UNREVEALED_PIXEL);
}

#[test]
fn debug_reveal_shows_generated_streamed_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let target_chunk = ChunkCoord { x: 0, y: 13 };
    let target = first_walkable_tile_in_chunk(sim.seed(), target_chunk);
    move_player_to_tile(&mut sim, target);
    let sample = (
        target_chunk.x * CHUNK_SIZE + (target.0 + 1).rem_euclid(CHUNK_SIZE),
        target.1,
    );

    let map = generate_map_pixels(
        &sim,
        &MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: false,
        },
    );

    assert_ne!(pixel_at(&map, sample), UNREVEALED_PIXEL);
}

fn first_walkable_tile_in_chunk(seed: u64, coord: ChunkCoord) -> (i32, i32) {
    let mut world = WorldSim::new_seeded(seed);
    world.ensure_chunk_generated(coord);
    for y in coord.y * CHUNK_SIZE..(coord.y + 1) * CHUNK_SIZE {
        for x in coord.x * CHUNK_SIZE..(coord.x + 1) * CHUNK_SIZE {
            if world
                .tile_at(x, y)
                .is_some_and(|tile| tile.collision.walkable)
            {
                return (x, y);
            }
        }
    }

    panic!("expected a walkable streamed tile");
}

fn move_player_to_tile(sim: &mut Simulation, tile: (i32, i32)) {
    let (player_x, player_y) = sim.player().position_tiles();
    sim.move_player_by_tiles(
        tile.0 as f32 + 0.5 - player_x,
        tile.1 as f32 + 0.5 - player_y,
    );
    assert_eq!(sim.player().tile_position(), tile);
}
