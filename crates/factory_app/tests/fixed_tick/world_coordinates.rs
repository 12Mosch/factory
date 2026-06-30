use bevy::prelude::*;
use factory_app::interaction::cursor::world_position_to_tile_coord;

#[test]
fn world_position_to_tile_coord_floors_negative_coordinates() {
    assert_eq!(world_position_to_tile_coord(Vec2::new(0.0, 0.0)), (0, 0));
    assert_eq!(world_position_to_tile_coord(Vec2::new(7.99, 7.99)), (0, 0));
    assert_eq!(world_position_to_tile_coord(Vec2::new(8.0, 8.0)), (1, 1));
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-0.01, -0.01)),
        (-1, -1)
    );
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-8.0, -8.0)),
        (-1, -1)
    );
    assert_eq!(
        world_position_to_tile_coord(Vec2::new(-8.01, -8.01)),
        (-2, -2)
    );
}
