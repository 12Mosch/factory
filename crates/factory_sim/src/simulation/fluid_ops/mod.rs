mod equalization;
mod geometry;
mod machines;
mod math;
mod network_access;
mod network_builder;
mod types;

#[allow(unused_imports)]
pub(in crate::simulation) use math::{ceil_div_u64, per_tick_milliunits};
pub(in crate::simulation) use types::{FluidBoxAssignment, FluidBoxKey, FluidNetworkTopology};

use geometry::{
    FluidConnectionGeometry, rotated_fluid_connection_geometry, rotated_fluid_endpoint,
};

use super::*;

impl Simulation {
    pub(super) fn invalidate_fluid_state(&mut self) {
        self.fluids.clear_networks();
    }

    /// For each cardinal direction (indexed by [`Direction::index`]), whether `entity_id` has
    /// a fluid connection joined to a matching connection on the adjacent entity.
    pub(in crate::simulation) fn fluid_connection_directions(
        &self,
        entity_id: EntityId,
    ) -> [bool; 4] {
        let mut connected = [false; 4];
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return connected;
        };
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
            return connected;
        };

        for connection in prototype
            .fluid_boxes
            .iter()
            .flat_map(|fluid_box| &fluid_box.connections)
        {
            let Some(geometry) = rotated_fluid_connection_geometry(placed, prototype, connection)
            else {
                continue;
            };
            let Some(direction) = tile_step_direction(geometry.tile, geometry.facing_tile) else {
                continue;
            };
            if !connected[direction.index()] {
                connected[direction.index()] =
                    self.fluid_connection_joins_neighbor(entity_id, geometry);
            }
        }
        connected
    }

    fn fluid_connection_joins_neighbor(
        &self,
        entity_id: EntityId,
        geometry: FluidConnectionGeometry,
    ) -> bool {
        let (facing_x, facing_y) = geometry.facing_tile;
        let Some(neighbor_id) = self.entities.occupancy.entity_at(facing_x, facing_y) else {
            return false;
        };
        if neighbor_id == entity_id || !self.entities.fluid_boxes.contains_key(&neighbor_id) {
            return false;
        }
        let Some(neighbor) = self.entities.placed_entity(neighbor_id) else {
            return false;
        };
        let Some(neighbor_prototype) = self.world.prototypes.entity(neighbor.prototype_id) else {
            return false;
        };

        neighbor_prototype
            .fluid_boxes
            .iter()
            .flat_map(|fluid_box| &fluid_box.connections)
            .any(|connection| {
                rotated_fluid_endpoint(neighbor, neighbor_prototype, connection)
                    == Some(geometry.endpoint)
            })
    }

    #[cfg(test)]
    pub(super) fn fluid_topology_rebuild_count(&self) -> u64 {
        self.fluids.topology_rebuilds
    }
}

fn tile_step_direction(
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
