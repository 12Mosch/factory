use crate::robots::{EntityRoboportStatus, RobotNetworkRoboportSnapshot, TileBounds};
use crate::simulation::*;

use super::types::RobotNetworkTopology;

impl Simulation {
    /// Network `entity_id` belongs to, or `None` when it is not a roboport.
    ///
    /// Answered from the settled topology, so between a placement and the robot
    /// pass that follows it this reports `None` rather than a stale network id.
    /// Presentation reads this every frame and would otherwise draw a network
    /// that no longer exists; simulation callers run after
    /// [`Simulation::ensure_robot_network_topology`] and always see the rebuilt
    /// answer.
    pub fn robot_network_id_for_entity(&self, entity_id: EntityId) -> Option<u32> {
        self.robots.network_ids_by_entity.get(&entity_id).copied()
    }

    /// Settled robot networks, one snapshot per network.
    pub fn robot_networks(&self) -> &[RobotNetworkSnapshot] {
        &self.robots.networks
    }

    /// The network whose construction coverage contains `(x, y)`, or `None`
    /// when no roboport reaches the tile.
    ///
    /// Coverage is the union of the member construction squares, so this walks
    /// the squares rather than testing the network bounding box: an L-shaped
    /// network must not claim the empty corner of its bounding box. Ties go to
    /// the lowest network id so the answer never depends on iteration order.
    ///
    /// Like [`Simulation::robot_network_id_for_entity`], this reads the settled
    /// topology and reports nothing while a rebuild is pending.
    pub fn construction_network_covering_tile(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<u32> {
        self.robots
            .topology_networks
            .iter()
            .find(|network| {
                network.construction_bounds.contains(x, y)
                    && network
                        .roboports
                        .iter()
                        .any(|roboport| roboport.construction_bounds.contains(x, y))
            })
            .map(|network| network.network_id)
    }

    /// Construction and logistic squares of one roboport, in world tiles.
    ///
    /// Answered straight from the prototype rather than from the topology cache
    /// so the build preview can show the coverage of a roboport that is only
    /// being *considered*, not yet placed.
    pub fn roboport_coverage_bounds_for_footprint(
        &self,
        prototype_id: EntityPrototypeId,
        footprint: EntityFootprint,
    ) -> Option<(TileBounds, TileBounds)> {
        let roboport = self.world.prototypes.entity(prototype_id)?.roboport?;
        Some((
            coverage_bounds(footprint, roboport.construction_radius_tiles),
            coverage_bounds(footprint, roboport.logistic_radius_tiles),
        ))
    }

    /// Robot-network status of one entity, or `None` when it is not a roboport.
    pub fn entity_roboport_status(&self, entity_id: EntityId) -> Option<EntityRoboportStatus> {
        let state = self.entities.roboports.get(&entity_id)?;
        let placed = self.entities.placed_entity(entity_id)?;
        let (construction_bounds, logistic_bounds) =
            self.roboport_coverage_bounds_for_footprint(placed.prototype_id, placed.footprint)?;
        Some(EntityRoboportStatus {
            network_id: self.robot_network_id_for_entity(entity_id),
            charge_energy_joules: state.charge_energy_joules,
            charge_capacity_joules: self.roboport_charge_capacity_joules(entity_id),
            construction_bounds,
            logistic_bounds,
        })
    }

    pub(in crate::simulation) fn roboport_prototype(
        &self,
        entity_id: EntityId,
    ) -> Option<factory_data::RoboportPrototype> {
        let placed = self.entities.placed_entity(entity_id)?;
        self.world.prototypes.entity(placed.prototype_id)?.roboport
    }

    pub(in crate::simulation) fn roboport_charge_capacity_joules(
        &self,
        entity_id: EntityId,
    ) -> u64 {
        self.roboport_prototype(entity_id)
            .map_or(0, |roboport| roboport.charging_energy_buffer_joules)
    }

    #[cfg(test)]
    pub(in crate::simulation) fn robot_topology_rebuild_count(&self) -> u64 {
        self.robots.topology_rebuilds
    }
}

/// Square a roboport radius covers around `footprint`, as inclusive tile
/// bounds.
pub(in crate::simulation) fn coverage_bounds(
    footprint: EntityFootprint,
    radius_tiles: u16,
) -> TileBounds {
    let (min_x, min_y, max_x, max_y) = factory_data::roboport_coverage_bounds(
        footprint.x,
        footprint.y,
        footprint.width,
        footprint.height,
        radius_tiles,
    );
    TileBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

pub(super) fn update_robot_network_snapshot(
    entities: &EntityStore,
    network: &RobotNetworkTopology,
    snapshot: &mut RobotNetworkSnapshot,
) {
    snapshot.network_id = network.network_id;
    snapshot.construction_bounds = network.construction_bounds;
    snapshot.logistic_bounds = network.logistic_bounds;
    snapshot.charge_capacity_joules = network.charge_capacity_joules;

    let mut charge_energy_joules = 0_u64;
    let mut snapshot_index = 0;
    for roboport in &network.roboports {
        let Some(state) = entities.roboports.get(&roboport.entity_id) else {
            continue;
        };
        charge_energy_joules = charge_energy_joules.saturating_add(state.charge_energy_joules);
        let roboport_snapshot = RobotNetworkRoboportSnapshot {
            entity_id: roboport.entity_id,
            construction_bounds: roboport.construction_bounds,
            logistic_bounds: roboport.logistic_bounds,
            charge_energy_joules: state.charge_energy_joules,
            charge_capacity_joules: roboport.charge_capacity_joules,
        };
        if let Some(existing) = snapshot.roboports.get_mut(snapshot_index) {
            *existing = roboport_snapshot;
        } else {
            snapshot.roboports.push(roboport_snapshot);
        }
        snapshot_index += 1;
    }
    snapshot.roboports.truncate(snapshot_index);
    snapshot.charge_energy_joules = charge_energy_joules;
}
