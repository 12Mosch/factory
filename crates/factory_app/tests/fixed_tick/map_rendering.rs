use super::common::{entity_id_by_name, pixel_at};
use bevy::prelude::Vec2;
use factory_app::map::resources::{MapDisplaySettings, MapLayer, MapTextureBounds};
use factory_app::rendering::map_texture::{
    GRID_PIXEL, MapPixels, UNREVEALED_PIXEL, generate_map_pixels, generate_map_pixels_for_layer,
};
use factory_app::ui::map_view::fullscreen_crop_bounds;
use factory_sim::{CHUNK_SIZE, ChunkCoord, Direction, Simulation, WorldSim};

#[test]
fn map_pixel_generation_draws_revealed_tiles_and_debug_grid() {
    let sim = Simulation::new_test_world(123);
    let normal = generate_map_pixels(&sim, &MapDisplaySettings::default());
    let player_tile = sim.player().tile_position();

    assert_ne!(pixel_at(&normal, player_tile), UNREVEALED_PIXEL);

    let unrevealed_chunk = sim
        .world()
        .chunks
        .keys()
        .copied()
        .find(|coord| !sim.is_chunk_revealed(*coord))
        .expect("initial chart should leave distant chunks unrevealed");
    assert!(!normal.bounds.contains_chunk(unrevealed_chunk));

    let debug = generate_map_pixels(
        &sim,
        &MapDisplaySettings {
            debug_reveal_all: true,
            show_chunk_grid: true,
        },
    );
    assert!(debug.bounds.contains_chunk(unrevealed_chunk));
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

    assert!(!map.bounds.contains_tile(sample));
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

#[test]
fn fullscreen_crop_bounds_clamps_near_map_edges() {
    let map_bounds = MapTextureBounds {
        min_x: -100,
        min_y: -50,
        width: 200,
        height: 100,
    };

    let lower_left = fullscreen_crop_bounds(map_bounds, Vec2::new(-500.0, -500.0), 1.0, Vec2::ONE);
    assert_eq!(lower_left.min_x, -100);
    assert_eq!(lower_left.min_y, -50);
    assert!(lower_left.width <= map_bounds.width);
    assert!(lower_left.height <= map_bounds.height);

    let upper_right = fullscreen_crop_bounds(map_bounds, Vec2::new(500.0, 500.0), 2.0, Vec2::ONE);
    assert_eq!(
        upper_right.min_x + i64::from(upper_right.width),
        map_bounds.min_x + i64::from(map_bounds.width)
    );
    assert_eq!(
        upper_right.min_y + i64::from(upper_right.height),
        map_bounds.min_y + i64::from(map_bounds.height)
    );
}

#[test]
fn map_layers_emphasize_resources_and_entities_without_revealing_hidden_chunks() {
    let mut sim = Simulation::new_test_world(123);
    let chest = entity_id_by_name(sim.catalog(), "chest");
    let (entity_x, entity_y) = revealed_buildable_tile(&sim, chest);
    let surface_before =
        generate_map_pixels_for_layer(&sim, &MapDisplaySettings::default(), MapLayer::Surface);
    let entities_before =
        generate_map_pixels_for_layer(&sim, &MapDisplaySettings::default(), MapLayer::Entities);
    factory_sim::placement::place(
        &mut sim,
        factory_sim::placement::EntityPlacementRequest {
            prototype_id: chest,
            x: entity_x,
            y: entity_y,
            direction: Direction::North,
        },
    )
    .expect("test chest should be placeable");
    let resource_tile = revealed_resource_tile(&sim);
    let concealed_chunk = ChunkCoord { x: 2, y: 0 };
    let far_revealed_chunk = ChunkCoord { x: 4, y: 0 };
    let target = first_walkable_tile_in_chunk(sim.seed(), far_revealed_chunk);
    move_player_to_tile(&mut sim, target);
    sim.tick();

    let surface =
        generate_map_pixels_for_layer(&sim, &MapDisplaySettings::default(), MapLayer::Surface);
    let resources =
        generate_map_pixels_for_layer(&sim, &MapDisplaySettings::default(), MapLayer::Resources);
    let entities =
        generate_map_pixels_for_layer(&sim, &MapDisplaySettings::default(), MapLayer::Entities);

    assert_ne!(
        pixel_at(&resources, resource_tile),
        pixel_at(&entities, resource_tile)
    );
    assert_eq!(
        pixel_at(&surface, (entity_x, entity_y)),
        pixel_at(&surface_before, (entity_x, entity_y))
    );
    assert_eq!(
        pixel_at(&entities, (entity_x, entity_y)),
        pixel_at(&entities_before, (entity_x, entity_y))
    );

    assert!(!sim.is_chunk_revealed(concealed_chunk));
    let hidden_tile = concealed_chunk.tile_at(1, 1);
    assert_hidden_pixel(&resources, hidden_tile);
    assert_hidden_pixel(&entities, hidden_tile);
}

fn assert_hidden_pixel(map: &MapPixels, tile: (i64, i64)) {
    assert!(map.bounds.contains_tile(tile));
    assert_eq!(pixel_at(map, tile), UNREVEALED_PIXEL);
}

fn first_walkable_tile_in_chunk(seed: u64, coord: ChunkCoord) -> (i64, i64) {
    let mut world = WorldSim::new_seeded(seed);
    world.ensure_chunk_generated(coord);
    let (min_x, min_y) = coord.min_tile();
    for y in min_y..min_y + i64::from(CHUNK_SIZE) {
        for x in min_x..min_x + i64::from(CHUNK_SIZE) {
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

fn move_player_to_tile(sim: &mut Simulation, tile: (i64, i64)) {
    let (player_x, player_y) = sim.player().position_tiles();
    sim.move_player_by_tiles(
        tile.0 as f32 + 0.5 - player_x,
        tile.1 as f32 + 0.5 - player_y,
    );
    assert_eq!(sim.player().tile_position(), tile);
}

fn revealed_resource_tile(sim: &Simulation) -> (i64, i64) {
    sim.world()
        .chunks
        .values()
        .flat_map(|chunk| {
            chunk
                .tiles
                .iter()
                .enumerate()
                .filter_map(move |(index, tile)| {
                    tile.resource?;
                    if !sim.is_chunk_revealed(chunk.coord) {
                        return None;
                    }
                    let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
                    let local_y = (index as i32).div_euclid(CHUNK_SIZE);
                    Some((
                        chunk.coord.x * CHUNK_SIZE + local_x,
                        chunk.coord.y * CHUNK_SIZE + local_y,
                    ))
                })
        })
        .next()
        .expect("generated revealed chunks should contain a resource tile")
}

fn revealed_buildable_tile(
    sim: &Simulation,
    prototype_id: factory_data::EntityPrototypeId,
) -> (i64, i64) {
    for chunk in sim.world().chunks.values() {
        if !sim.is_chunk_revealed(chunk.coord) {
            continue;
        }
        for (index, _) in chunk.tiles.iter().enumerate() {
            let local_x = (index as i32).rem_euclid(CHUNK_SIZE);
            let local_y = (index as i32).div_euclid(CHUNK_SIZE);
            let x = chunk.coord.x * CHUNK_SIZE + local_x;
            let y = chunk.coord.y * CHUNK_SIZE + local_y;
            if factory_sim::placement::validate(
                sim,
                factory_sim::placement::EntityPlacementRequest {
                    prototype_id,
                    x,
                    y,
                    direction: Direction::North,
                },
            )
            .is_ok()
            {
                return (x, y);
            }
        }
    }

    panic!("expected a revealed buildable tile");
}
