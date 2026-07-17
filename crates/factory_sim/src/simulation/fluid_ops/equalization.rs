use crate::simulation::*;

use super::geometry::rotated_fluid_endpoint;
use super::math::proportional_amount;
use super::network_access::{fluid_network_dynamic_summary, update_fluid_network_snapshot};
use super::network_builder::build_fluid_network_topology_from_nodes;
use super::types::{FluidBoxAssignment, FluidBoxNode, FluidNetworkTopology};

impl Simulation {
    pub(in crate::simulation) fn ensure_fluid_network_topology(&mut self) {
        if !self.fluids.topology_dirty {
            return;
        }

        let topology_networks = self.build_fluid_network_topology();
        self.fluids.replace_topology(topology_networks);
        #[cfg(test)]
        {
            self.fluids.topology_rebuilds += 1;
        }
    }

    pub(in crate::simulation) fn equalize_fluid_networks(&mut self) {
        self.ensure_fluid_network_topology();
        for network_index in 0..self.fluids.topology_networks.len() {
            if !self.fluids.networks_needing_equalization[network_index] {
                continue;
            }
            self.fluids.networks_needing_equalization[network_index] = false;
            self.fluids.networks_needing_snapshot[network_index] = true;
            equalize_fluid_network(
                &mut self.entities,
                &self.fluids.topology_networks[network_index],
                &mut self.fluids.equalization_assignments,
            );
        }
    }

    pub(in crate::simulation) fn refresh_fluid_network_snapshots(&mut self) {
        self.ensure_fluid_network_topology();
        for network_index in 0..self.fluids.topology_networks.len() {
            if !self.fluids.networks_needing_snapshot[network_index] {
                continue;
            }
            self.fluids.networks_needing_snapshot[network_index] = false;
            if self.fluids.networks.len() == network_index {
                self.fluids.networks.push(FluidNetworkSnapshot::default());
            }
            update_fluid_network_snapshot(
                &self.entities,
                &self.fluids.topology_networks[network_index],
                &mut self.fluids.networks[network_index],
            );
        }
        self.fluids
            .networks
            .truncate(self.fluids.topology_networks.len());
    }

    pub(in crate::simulation) fn refresh_fluid_networks_after_dynamic_changes(&mut self) {
        self.equalize_fluid_networks();
        self.refresh_fluid_network_snapshots();
    }

    fn build_fluid_network_topology(&self) -> Vec<FluidNetworkTopology> {
        let nodes = self.fluid_box_nodes();
        build_fluid_network_topology_from_nodes(&nodes)
    }

    fn fluid_box_nodes(&self) -> Vec<FluidBoxNode> {
        let mut nodes = Vec::new();
        for placed in self.entities.placed_entities.values() {
            if !self.entities.fluid_boxes.contains_key(&placed.id) {
                continue;
            }
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };

            for (box_index, fluid_box) in prototype.fluid_boxes.iter().enumerate() {
                let endpoints = fluid_box
                    .connections
                    .iter()
                    .filter_map(|connection| rotated_fluid_endpoint(placed, prototype, connection))
                    .collect();
                nodes.push(FluidBoxNode {
                    key: FluidBoxKey {
                        entity_id: placed.id,
                        box_index,
                    },
                    capacity_milliunits: fluid_box.capacity_milliunits,
                    filter: fluid_box.filter,
                    endpoints,
                });
            }
        }
        nodes
    }
}

fn equalize_fluid_network(
    entities: &mut EntityStore,
    network: &FluidNetworkTopology,
    assignments: &mut Vec<FluidBoxAssignment>,
) {
    if network.boxes.is_empty() || network.capacity_milliunits == 0 {
        return;
    }

    let summary = fluid_network_dynamic_summary(entities, network);
    if summary.blocked {
        return;
    }

    if summary.total_milliunits == 0 {
        for box_topology in &network.boxes {
            let key = box_topology.key;
            if let Some(state) = entities
                .fluid_boxes
                .get_mut(&key.entity_id)
                .and_then(|boxes| boxes.get_mut(key.box_index))
            {
                state.amount_milliunits = 0;
                state.fluid_id = None;
            }
        }
        return;
    }
    let Some(fluid_id) = summary.fluid_id else {
        return;
    };

    assignments.clear();
    let mut assigned_total = 0_u64;
    for box_topology in &network.boxes {
        let key = box_topology.key;
        let capacity = box_topology.capacity_milliunits;
        let assigned = proportional_amount(
            summary.total_milliunits,
            capacity,
            network.capacity_milliunits,
        );
        assignments.push(FluidBoxAssignment {
            key,
            capacity_milliunits: capacity,
            amount_milliunits: assigned,
        });
        assigned_total = assigned_total.saturating_add(assigned);
    }

    let mut remainder = summary.total_milliunits.saturating_sub(assigned_total);
    for assignment in assignments.iter_mut() {
        if remainder == 0 {
            break;
        }
        if assignment.amount_milliunits < assignment.capacity_milliunits {
            assignment.amount_milliunits += 1;
            remainder -= 1;
        }
    }

    debug_assert_eq!(remainder, 0);
    for assignment in assignments.iter() {
        let key = assignment.key;
        let assigned = assignment.amount_milliunits;
        if let Some(state) = entities
            .fluid_boxes
            .get_mut(&key.entity_id)
            .and_then(|boxes| boxes.get_mut(key.box_index))
        {
            state.amount_milliunits = assigned;
            state.fluid_id = (assigned > 0).then_some(fluid_id);
        }
    }
}
