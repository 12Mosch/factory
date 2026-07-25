//! Tile-edge connection geometry shared by fluid boxes and heat buffers.
//!
//! Both networks join neighbours the same way: a prototype declares openings on
//! footprint edges, and two entities are connected when their openings meet on
//! the same edge of the tile grid. Rotating a declared opening into world space
//! and naming the shared edge is therefore one problem, solved once here.

use crate::simulation::{Direction, PlacedEntity, WorldTileCoord};
use factory_data::{ConnectionSide, EdgeConnectionPrototype, EntityPrototype};

/// The edge two entities share when their openings meet. Horizontal edges at
/// `y` separate rows `y - 1` and `y`; vertical edges at `x` separate columns
/// `x - 1` and `x`. Equality of endpoints is exactly network adjacency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) struct EdgeEndpoint {
    pub(in crate::simulation) x: WorldTileCoord,
    pub(in crate::simulation) y: WorldTileCoord,
    pub(in crate::simulation) axis: EdgeEndpointAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::simulation) enum EdgeEndpointAxis {
    Horizontal,
    Vertical,
}

/// A connection resolved into world space: the shared-edge endpoint, the tile
/// the connection sits on, and the adjacent tile it opens toward.
#[derive(Clone, Copy, Debug)]
pub(in crate::simulation) struct EdgeConnectionGeometry {
    pub(in crate::simulation) endpoint: EdgeEndpoint,
    pub(in crate::simulation) tile: (WorldTileCoord, WorldTileCoord),
    pub(in crate::simulation) facing_tile: (WorldTileCoord, WorldTileCoord),
}

pub(in crate::simulation) fn rotated_edge_endpoint(
    placed: &PlacedEntity,
    prototype: &EntityPrototype,
    connection: &EdgeConnectionPrototype,
) -> Option<EdgeEndpoint> {
    Some(rotated_edge_connection_geometry(placed, prototype, connection)?.endpoint)
}

pub(in crate::simulation) fn rotated_edge_connection_geometry(
    placed: &PlacedEntity,
    prototype: &EntityPrototype,
    connection: &EdgeConnectionPrototype,
) -> Option<EdgeConnectionGeometry> {
    let (local_x, local_y, side) = rotate_edge_connection(
        connection.local_offset.x,
        connection.local_offset.y,
        connection.side,
        prototype.size.x,
        prototype.size.y,
        placed.direction,
    )?;
    let tile_x = placed.footprint.x + i64::from(local_x);
    let tile_y = placed.footprint.y + i64::from(local_y);

    Some(EdgeConnectionGeometry {
        endpoint: endpoint_for_side(tile_x, tile_y, side),
        tile: (tile_x, tile_y),
        facing_tile: facing_tile_for_side(tile_x, tile_y, side),
    })
}

/// The cardinal step from `from` to `to`, or `None` when they are not adjacent.
pub(in crate::simulation) fn tile_step_direction(
    from: (WorldTileCoord, WorldTileCoord),
    to: (WorldTileCoord, WorldTileCoord),
) -> Option<Direction> {
    match (to.0 - from.0, to.1 - from.1) {
        (0, 1) => Some(Direction::North),
        (1, 0) => Some(Direction::East),
        (0, -1) => Some(Direction::South),
        (-1, 0) => Some(Direction::West),
        _ => None,
    }
}

fn rotate_edge_connection(
    local_x: i32,
    local_y: i32,
    side: ConnectionSide,
    width: i32,
    height: i32,
    direction: Direction,
) -> Option<(i32, i32, ConnectionSide)> {
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
    side: ConnectionSide,
) -> EdgeEndpoint {
    match side {
        ConnectionSide::North => EdgeEndpoint {
            x: tile_x,
            y: tile_y,
            axis: EdgeEndpointAxis::Horizontal,
        },
        ConnectionSide::East => EdgeEndpoint {
            x: tile_x + 1,
            y: tile_y,
            axis: EdgeEndpointAxis::Vertical,
        },
        ConnectionSide::South => EdgeEndpoint {
            x: tile_x,
            y: tile_y + 1,
            axis: EdgeEndpointAxis::Horizontal,
        },
        ConnectionSide::West => EdgeEndpoint {
            x: tile_x,
            y: tile_y,
            axis: EdgeEndpointAxis::Vertical,
        },
    }
}

/// The tile on the far side of the edge a connection opens toward.
fn facing_tile_for_side(
    tile_x: WorldTileCoord,
    tile_y: WorldTileCoord,
    side: ConnectionSide,
) -> (WorldTileCoord, WorldTileCoord) {
    match side {
        ConnectionSide::North => (tile_x, tile_y - 1),
        ConnectionSide::East => (tile_x + 1, tile_y),
        ConnectionSide::South => (tile_x, tile_y + 1),
        ConnectionSide::West => (tile_x - 1, tile_y),
    }
}

fn rotate_side_clockwise(side: ConnectionSide) -> ConnectionSide {
    match side {
        ConnectionSide::North => ConnectionSide::East,
        ConnectionSide::East => ConnectionSide::South,
        ConnectionSide::South => ConnectionSide::West,
        ConnectionSide::West => ConnectionSide::North,
    }
}

fn rotate_side_counter_clockwise(side: ConnectionSide) -> ConnectionSide {
    match side {
        ConnectionSide::North => ConnectionSide::West,
        ConnectionSide::West => ConnectionSide::South,
        ConnectionSide::South => ConnectionSide::East,
        ConnectionSide::East => ConnectionSide::North,
    }
}

fn opposite_side(side: ConnectionSide) -> ConnectionSide {
    match side {
        ConnectionSide::North => ConnectionSide::South,
        ConnectionSide::East => ConnectionSide::West,
        ConnectionSide::South => ConnectionSide::North,
        ConnectionSide::West => ConnectionSide::East,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_preserve_current_coordinate_behavior() {
        assert_eq!(
            rotate_edge_connection(1, 2, ConnectionSide::North, 3, 4, Direction::North),
            Some((1, 2, ConnectionSide::North))
        );
        assert_eq!(
            rotate_edge_connection(1, 2, ConnectionSide::North, 3, 4, Direction::East),
            Some((1, 1, ConnectionSide::East))
        );
        assert_eq!(
            rotate_edge_connection(1, 2, ConnectionSide::North, 3, 4, Direction::South),
            Some((1, 1, ConnectionSide::South))
        );
        assert_eq!(
            rotate_edge_connection(1, 2, ConnectionSide::North, 3, 4, Direction::West),
            Some((2, 1, ConnectionSide::West))
        );
    }

    #[test]
    fn out_of_bounds_local_connections_return_none() {
        assert_eq!(
            rotate_edge_connection(-1, 0, ConnectionSide::North, 3, 4, Direction::North),
            None
        );
        assert_eq!(
            rotate_edge_connection(0, -1, ConnectionSide::North, 3, 4, Direction::North),
            None
        );
        assert_eq!(
            rotate_edge_connection(3, 0, ConnectionSide::North, 3, 4, Direction::North),
            None
        );
        assert_eq!(
            rotate_edge_connection(0, 4, ConnectionSide::North, 3, 4, Direction::North),
            None
        );
    }

    #[test]
    fn endpoint_axis_selection_matches_side_orientation() {
        assert_eq!(
            endpoint_for_side(10, 20, ConnectionSide::North).axis,
            EdgeEndpointAxis::Horizontal
        );
        assert_eq!(
            endpoint_for_side(10, 20, ConnectionSide::South).axis,
            EdgeEndpointAxis::Horizontal
        );
        assert_eq!(
            endpoint_for_side(10, 20, ConnectionSide::East).axis,
            EdgeEndpointAxis::Vertical
        );
        assert_eq!(
            endpoint_for_side(10, 20, ConnectionSide::West).axis,
            EdgeEndpointAxis::Vertical
        );
    }

    /// Two entities are on the same network exactly when their openings resolve
    /// to the same endpoint, so opposite openings across one edge must match.
    #[test]
    fn facing_openings_across_one_edge_share_an_endpoint() {
        assert_eq!(
            endpoint_for_side(10, 20, ConnectionSide::East),
            endpoint_for_side(11, 20, ConnectionSide::West)
        );
        assert_eq!(
            endpoint_for_side(10, 20, ConnectionSide::South),
            endpoint_for_side(10, 21, ConnectionSide::North)
        );
    }
}
