use super::common::pixel_at;
use factory_app::rendering::map_texture::{
    GRID_PIXEL, PLAYER_PIXEL, UNREVEALED_PIXEL, generate_map_pixels,
};
use factory_app::resources::MapDisplaySettings;
use factory_sim::{CHUNK_SIZE, Simulation};

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
