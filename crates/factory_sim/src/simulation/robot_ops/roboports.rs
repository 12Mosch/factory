use crate::simulation::*;

use super::network_access::{coverage_bounds, update_robot_network_snapshot};
use super::network_builder::build_robot_network_topology_from_nodes;
use super::types::{RoboportNode, RobotNetworkTopology};

impl Simulation {
    /// Advances robot networks for one tick: settle the topology, top up the
    /// roboport charging buffers, then refresh the durable snapshots.
    ///
    /// Nothing flows between roboports, so there is no equalization step here;
    /// each roboport fills its own buffer from the electric network it is
    /// connected to, and the network snapshot only sums the results.
    pub(in crate::simulation) fn advance_robot_networks(&mut self) {
        self.ensure_robot_network_topology();
        self.charge_roboport_buffers();
        self.refresh_robot_network_snapshots();
    }

    pub(in crate::simulation) fn ensure_robot_network_topology(&mut self) {
        if !self.robots.topology_dirty {
            return;
        }

        let topology_networks = self.build_robot_network_topology();
        self.robots.replace_topology(topology_networks);
        self.rebuild_construction_job_routing();
        #[cfg(test)]
        {
            self.robots.topology_rebuilds += 1;
        }
    }

    pub(in crate::simulation) fn refresh_robot_network_snapshots(&mut self) {
        self.ensure_robot_network_topology();
        self.robots.networks.resize(
            self.robots.topology_networks.len(),
            RobotNetworkSnapshot::default(),
        );
        for network_index in 0..self.robots.topology_networks.len() {
            if !self.robots.networks_needing_snapshot[network_index] {
                continue;
            }
            self.robots.networks_needing_snapshot[network_index] = false;
            update_robot_network_snapshot(
                &self.entities,
                &self.robots.topology_networks[network_index],
                &mut self.robots.networks[network_index],
            );
        }
        self.refresh_robot_network_work_counts();
    }

    /// Fills each roboport's charging buffer from its electric network.
    ///
    /// The buffer is what robots will charge from, so filling it is ordinary
    /// electric-consumer work: the roboport asks for a work grant, and a
    /// partially powered network slows the fill through the shared satisfaction
    /// remainder rather than delivering a fraction of a joule per tick. A full
    /// buffer stops asking, which is what drops a settled roboport back to its
    /// idle drain.
    fn charge_roboport_buffers(&mut self) {
        // Moved out of `self` so the loop body can mutably borrow the entity
        // store and the subsystem; handed back at the end so the next tick
        // reuses the allocation.
        let mut roboport_ids = std::mem::take(&mut self.robots.charging_scratch);
        roboport_ids.clear();
        roboport_ids.extend(self.entities.roboports.keys().copied());

        for &entity_id in &roboport_ids {
            let capacity = self.roboport_charge_capacity_joules(entity_id);
            let Some(charge_watts) = self.roboport_charge_watts(entity_id) else {
                continue;
            };
            let stored = self
                .entities
                .roboports
                .get(&entity_id)
                .map_or(0, |state| state.charge_energy_joules);
            debug_assert_eq!(
                stored < capacity,
                roboport_is_charging(&self.world.prototypes, &self.entities, entity_id),
                "the charging predicate the power model reads must match what actually charges"
            );
            if stored >= capacity {
                continue;
            }
            if !electric_work_allowed_for(
                &self.power,
                &mut self.entities.electric_consumers,
                entity_id,
            ) {
                continue;
            }

            let charged = (charge_watts / SIMULATION_TICKS_PER_SECOND)
                .max(1)
                .min(capacity - stored);
            if let Some(state) = self.entities.roboports.get_mut(&entity_id) {
                state.charge_energy_joules += charged;
            }
            self.robots.mark_roboport_dirty(entity_id);
        }

        self.robots.charging_scratch = roboport_ids;
    }

    /// Rate a roboport refills its buffer at, taken from the electric energy
    /// source it declares. A roboport without one cannot charge at all.
    fn roboport_charge_watts(&self, entity_id: EntityId) -> Option<u64> {
        let placed = self.entities.placed_entity(entity_id)?;
        let prototype = self.world.prototypes.entity(placed.prototype_id)?;
        prototype.roboport?;
        Some(
            prototype
                .electric_energy_source
                .as_ref()?
                .energy_usage_watts,
        )
    }

    fn build_robot_network_topology(&self) -> Vec<RobotNetworkTopology> {
        let nodes = self.roboport_nodes();
        build_robot_network_topology_from_nodes(&nodes)
    }

    fn roboport_nodes(&self) -> Vec<RoboportNode> {
        let mut nodes = Vec::with_capacity(self.entities.roboports.len());
        for placed in self.entities.placed_entities.values() {
            if !self.entities.roboports.contains_key(&placed.id) {
                continue;
            }
            let Some(roboport) = self
                .world
                .prototypes
                .entity(placed.prototype_id)
                .and_then(|prototype| prototype.roboport)
            else {
                continue;
            };

            nodes.push(RoboportNode {
                entity_id: placed.id,
                construction_bounds: coverage_bounds(
                    placed.footprint,
                    roboport.construction_radius_tiles,
                ),
                logistic_bounds: coverage_bounds(placed.footprint, roboport.logistic_radius_tiles),
                charge_capacity_joules: roboport.charging_energy_buffer_joules,
            });
        }
        nodes
    }
}

/// Whether `entity_id` is a roboport with room left in its charging buffer.
///
/// This is the roboport's answer to "can this consumer work?", so a full
/// roboport falls back to its idle drain instead of holding a charging-sized
/// claim on the network forever. Free-standing rather than a [`Simulation`]
/// method because the power demand model only carries the catalog and the
/// entity store.
pub(in crate::simulation) fn roboport_is_charging(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    entity_id: EntityId,
) -> bool {
    let Some(state) = entities.roboports.get(&entity_id) else {
        return false;
    };
    entities
        .placed_entity(entity_id)
        .and_then(|placed| catalog.entity(placed.prototype_id))
        .and_then(|prototype| prototype.roboport)
        .is_some_and(|roboport| state.charge_energy_joules < roboport.charging_energy_buffer_joules)
}
