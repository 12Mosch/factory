use crate::simulation::{Direction, PlacedEntity, WorldTileCoord};

use super::types::{FluidEndpoint, FluidEndpointAxis};

/// A fluid connection resolved into world space: the shared-edge endpoint, the tile the
/// connection sits on, and the adjacent tile it opens toward.
#[derive(Clone, Copy, Debug)]
pub(super) struct FluidConnectionGeometry {
    pub(super) endpoint: FluidEndpoint,
    pub(super) tile: (WorldTileCoord, WorldTileCoord),
    pub(super) facing_tile: (WorldTileCoord, WorldTileCoord),
}

pub(super) fn rotated_fluid_endpoint(
    placed: &PlacedEntity,
    prototype: &factory_data::EntityPrototype,
    connection: &factory_data::FluidConnectionPrototype,
) -> Option<FluidEndpoint> {
    Some(rotated_fluid_connection_geometry(placed, prototype, connection)?.endpoint)
}

pub(super) fn rotated_fluid_connection_geometry(
    placed: &PlacedEntity,
    prototype: &factory_data::EntityPrototype,
    connection: &factory_data::FluidConnectionPrototype,
) -> Option<FluidConnectionGeometry> {
    let (local_x, local_y, side) = rotate_fluid_connection(
        connection.local_offset.x,
        connection.local_offset.y,
        connection.side,
        prototype.size.x,
        prototype.size.y,
        placed.direction,
    )?;
    let tile_x = placed.footprint.x + i64::from(local_x);
    let tile_y = placed.footprint.y + i64::from(local_y);

    Some(FluidConnectionGeometry {
        endpoint: endpoint_for_side(tile_x, tile_y, side),
        tile: (tile_x, tile_y),
        facing_tile: facing_tile_for_side(tile_x, tile_y, side),
    })
}

fn rotate_fluid_connection(
    local_x: i32,
    local_y: i32,
    side: factory_data::FluidConnectionSide,
    width: i32,
    height: i32,
    direction: Direction,
) -> Option<(i32, i32, factory_data::FluidConnectionSide)> {
    if local_x < 0 || local_y < 0 || local_x >= width || local_y >= height {
        return None;
    }

    match direction {
        Direction::North => Some((local_x, local_y, side)),
        Direction::East => Some((height - 1 - local_y, local_x, rotate_side_clockwise(side))),
        Direction::South => Some((
            width - 1 - local_x,
            height - 1 - local_y,
            opposite_side(side),
        )),
        Direction::West => Some((
            local_y,
            width - 1 - local_x,
            rotate_side_counter_clockwise(side),
        )),
    }
}

fn endpoint_for_side(
    tile_x: WorldTileCoord,
    tile_y: WorldTileCoord,
    side: factory_data::FluidConnectionSide,
) -> FluidEndpoint {
    match side {
        factory_data::FluidConnectionSide::North => FluidEndpoint {
            x: tile_x,
            y: tile_y,
            axis: FluidEndpointAxis::Horizontal,
        },
        factory_data::FluidConnectionSide::East => FluidEndpoint {
            x: tile_x + 1,
            y: tile_y,
            axis: FluidEndpointAxis::Vertical,
        },
        factory_data::FluidConnectionSide::South => FluidEndpoint {
            x: tile_x,
            y: tile_y + 1,
            axis: FluidEndpointAxis::Horizontal,
        },
        factory_data::FluidConnectionSide::West => FluidEndpoint {
            x: tile_x,
            y: tile_y,
            axis: FluidEndpointAxis::Vertical,
        },
    }
}

/// The tile on the far side of the edge a connection opens toward. Horizontal edges at
/// `y` separate rows `y - 1` and `y`; vertical edges at `x` separate columns `x - 1` and `x`.
fn facing_tile_for_side(
    tile_x: WorldTileCoord,
    tile_y: WorldTileCoord,
    side: factory_data::FluidConnectionSide,
) -> (WorldTileCoord, WorldTileCoord) {
    match side {
        factory_data::FluidConnectionSide::North => (tile_x, tile_y - 1),
        factory_data::FluidConnectionSide::East => (tile_x + 1, tile_y),
        factory_data::FluidConnectionSide::South => (tile_x, tile_y + 1),
        factory_data::FluidConnectionSide::West => (tile_x - 1, tile_y),
    }
}

fn rotate_side_clockwise(
    side: factory_data::FluidConnectionSide,
) -> factory_data::FluidConnectionSide {
    match side {
        factory_data::FluidConnectionSide::North => factory_data::FluidConnectionSide::East,
        factory_data::FluidConnectionSide::East => factory_data::FluidConnectionSide::South,
        factory_data::FluidConnectionSide::South => factory_data::FluidConnectionSide::West,
        factory_data::FluidConnectionSide::West => factory_data::FluidConnectionSide::North,
    }
}

fn rotate_side_counter_clockwise(
    side: factory_data::FluidConnectionSide,
) -> factory_data::FluidConnectionSide {
    match side {
        factory_data::FluidConnectionSide::North => factory_data::FluidConnectionSide::West,
        factory_data::FluidConnectionSide::West => factory_data::FluidConnectionSide::South,
        factory_data::FluidConnectionSide::South => factory_data::FluidConnectionSide::East,
        factory_data::FluidConnectionSide::East => factory_data::FluidConnectionSide::North,
    }
}

fn opposite_side(side: factory_data::FluidConnectionSide) -> factory_data::FluidConnectionSide {
    match side {
        factory_data::FluidConnectionSide::North => factory_data::FluidConnectionSide::South,
        factory_data::FluidConnectionSide::East => factory_data::FluidConnectionSide::West,
        factory_data::FluidConnectionSide::South => factory_data::FluidConnectionSide::North,
        factory_data::FluidConnectionSide::West => factory_data::FluidConnectionSide::East,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use factory_data::FluidConnectionSide;

    #[test]
    fn rotations_preserve_current_coordinate_behavior() {
        assert_eq!(
            rotate_fluid_connection(1, 2, FluidConnectionSide::North, 3, 4, Direction::North),
            Some((1, 2, FluidConnectionSide::North))
        );
        assert_eq!(
            rotate_fluid_connection(1, 2, FluidConnectionSide::North, 3, 4, Direction::East),
            Some((1, 1, FluidConnectionSide::East))
        );
        assert_eq!(
            rotate_fluid_connection(1, 2, FluidConnectionSide::North, 3, 4, Direction::South),
            Some((1, 1, FluidConnectionSide::South))
        );
        assert_eq!(
            rotate_fluid_connection(1, 2, FluidConnectionSide::North, 3, 4, Direction::West),
            Some((2, 1, FluidConnectionSide::West))
        );
    }

    #[test]
    fn out_of_bounds_local_fluid_connections_return_none() {
        assert_eq!(
            rotate_fluid_connection(-1, 0, FluidConnectionSide::North, 3, 4, Direction::North),
            None
        );
        assert_eq!(
            rotate_fluid_connection(0, -1, FluidConnectionSide::North, 3, 4, Direction::North),
            None
        );
        assert_eq!(
            rotate_fluid_connection(3, 0, FluidConnectionSide::North, 3, 4, Direction::North),
            None
        );
        assert_eq!(
            rotate_fluid_connection(0, 4, FluidConnectionSide::North, 3, 4, Direction::North),
            None
        );
    }

    #[test]
    fn endpoint_axis_selection_matches_side_orientation() {
        assert_eq!(
            endpoint_for_side(10, 20, FluidConnectionSide::North).axis,
            FluidEndpointAxis::Horizontal
        );
        assert_eq!(
            endpoint_for_side(10, 20, FluidConnectionSide::South).axis,
            FluidEndpointAxis::Horizontal
        );
        assert_eq!(
            endpoint_for_side(10, 20, FluidConnectionSide::East).axis,
            FluidEndpointAxis::Vertical
        );
        assert_eq!(
            endpoint_for_side(10, 20, FluidConnectionSide::West).axis,
            FluidEndpointAxis::Vertical
        );
    }
}
