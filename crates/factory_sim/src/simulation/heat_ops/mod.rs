mod equalization;
mod machines;
mod network_access;
mod network_builder;
mod types;

pub(in crate::simulation) use types::HeatNetworkTopology;

use super::*;
use crate::simulation::edge_geometry::{
    EdgeConnectionGeometry, rotated_edge_connection_geometry, rotated_edge_endpoint,
    tile_step_direction,
};

impl Simulation {
    pub(super) fn invalidate_heat_state(&mut self) {
        self.heat.clear_networks();
    }

    /// For each cardinal direction (indexed by [`Direction::index`]), whether
    /// `entity_id` has a heat connection joined to a matching connection on the
    /// adjacent entity. Heat pipes render their arms from this, the same way
    /// pipes do for fluids.
    pub fn heat_connection_directions(&self, entity_id: EntityId) -> [bool; 4] {
        let mut connected = [false; 4];
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return connected;
        };
        let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
            return connected;
        };
        let Some(heat_buffer) = prototype.heat_buffer.as_ref() else {
            return connected;
        };

        for connection in &heat_buffer.connections {
            let Some(geometry) = rotated_edge_connection_geometry(placed, prototype, connection)
            else {
                continue;
            };
            let Some(direction) = tile_step_direction(geometry.tile, geometry.facing_tile) else {
                continue;
            };
            if !connected[direction.index()] {
                connected[direction.index()] =
                    self.heat_connection_joins_neighbor(entity_id, geometry);
            }
        }
        connected
    }

    fn heat_connection_joins_neighbor(
        &self,
        entity_id: EntityId,
        geometry: EdgeConnectionGeometry,
    ) -> bool {
        let (facing_x, facing_y) = geometry.facing_tile;
        let Some(neighbor_id) = self.entities.occupancy.entity_at(facing_x, facing_y) else {
            return false;
        };
        if neighbor_id == entity_id || !self.entities.heat_buffers.contains_key(&neighbor_id) {
            return false;
        }
        let Some(neighbor) = self.entities.placed_entity(neighbor_id) else {
            return false;
        };
        let Some(neighbor_prototype) = self.world.prototypes.entity(neighbor.prototype_id) else {
            return false;
        };

        neighbor_prototype
            .heat_buffer
            .as_ref()
            .is_some_and(|heat_buffer| {
                heat_buffer.connections.iter().any(|connection| {
                    rotated_edge_endpoint(neighbor, neighbor_prototype, connection)
                        == Some(geometry.endpoint)
                })
            })
    }
}
