use crate::simulation::*;

use super::geometry::rotated_fluid_endpoint;
use super::math::proportional_amount;
use super::network_builder::build_fluid_network_topology_from_nodes;
use super::types::{FluidBoxKey, FluidBoxNode, FluidNetworkTopology};

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
            self.equalize_fluid_network(network_index);
        }
    }

    pub(in crate::simulation) fn refresh_fluid_network_snapshots(&mut self) {
        self.ensure_fluid_network_topology();
        let networks = self
            .fluids
            .topology_networks
            .iter()
            .map(|network| self.fluid_network_snapshot(network))
            .collect();
        self.fluids.replace_networks(networks);
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

    fn equalize_fluid_network(&mut self, network_index: usize) {
        let Some(network) = self.fluids.topology_networks.get(network_index) else {
            return;
        };
        if network.boxes.is_empty() || network.capacity_milliunits == 0 {
            return;
        }

        let summary = self.fluid_network_dynamic_summary(network);
        if summary.blocked {
            return;
        }

        if summary.total_milliunits == 0 {
            let box_count = self.fluids.topology_networks[network_index].boxes.len();
            for box_index in 0..box_count {
                let key = self.fluids.topology_networks[network_index].boxes[box_index].key;
                if let Some(state) = self
                    .entities
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

        let mut assignments = Vec::with_capacity(network.boxes.len());
        let mut assigned_total = 0_u64;
        for box_topology in &network.boxes {
            let key = box_topology.key;
            let capacity = box_topology.capacity_milliunits;
            let assigned = proportional_amount(
                summary.total_milliunits,
                capacity,
                network.capacity_milliunits,
            );
            assignments.push((key, capacity, assigned));
            assigned_total = assigned_total.saturating_add(assigned);
        }

        let mut remainder = summary.total_milliunits.saturating_sub(assigned_total);
        for (_, capacity, assigned) in &mut assignments {
            if remainder == 0 {
                break;
            }
            if *assigned < *capacity {
                *assigned += 1;
                remainder -= 1;
            }
        }

        debug_assert_eq!(remainder, 0);
        for (key, _, assigned) in assignments {
            if let Some(state) = self
                .entities
                .fluid_boxes
                .get_mut(&key.entity_id)
                .and_then(|boxes| boxes.get_mut(key.box_index))
            {
                state.amount_milliunits = assigned;
                state.fluid_id = (assigned > 0).then_some(fluid_id);
            }
        }
    }
}
