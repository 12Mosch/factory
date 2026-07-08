use crate::simulation::*;

use super::geometry::rotated_fluid_endpoint;
use super::math::proportional_amount;
use super::network_builder::build_fluid_networks_from_nodes;
use super::types::{BuiltFluidNetwork, FluidBoxKey, FluidBoxNode};

impl Simulation {
    pub(in crate::simulation) fn rebuild_fluid_networks_and_equalize(&mut self) {
        let networks = self.build_fluid_networks();
        for network in &networks {
            if !network.blocked {
                self.equalize_fluid_network(network);
            }
        }

        let network_snapshots = networks
            .iter()
            .map(|network| self.fluid_network_snapshot(network))
            .collect();
        self.fluids.replace_networks(network_snapshots);
    }

    fn build_fluid_networks(&self) -> Vec<BuiltFluidNetwork> {
        let nodes = self.fluid_box_nodes();
        build_fluid_networks_from_nodes(&nodes)
    }

    fn fluid_box_nodes(&self) -> Vec<FluidBoxNode> {
        let mut nodes = Vec::new();
        for placed in self.entities.placed_entities.values() {
            let Some(entity_fluid_boxes) = self.entities.fluid_boxes.get(&placed.id) else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };

            for (box_index, fluid_box) in prototype.fluid_boxes.iter().enumerate() {
                let Some(state) = entity_fluid_boxes.get(box_index) else {
                    continue;
                };
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
                    amount_milliunits: state.amount_milliunits,
                    fluid_id: state.fluid_id,
                    endpoints,
                });
            }
        }
        nodes
    }

    fn equalize_fluid_network(&mut self, network: &BuiltFluidNetwork) {
        if network.boxes.is_empty() || network.capacity_milliunits == 0 {
            return;
        }

        let fluid_id = network.fluid_id;
        if network.total_milliunits == 0 {
            for key in &network.boxes {
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
        let Some(fluid_id) = fluid_id else {
            return;
        };

        let mut assignments = Vec::with_capacity(network.boxes.len());
        let mut assigned_total = 0_u64;
        for key in &network.boxes {
            let Some(capacity) = self.fluid_box_capacity(*key) else {
                continue;
            };
            let assigned = proportional_amount(
                network.total_milliunits,
                capacity,
                network.capacity_milliunits,
            );
            assignments.push((*key, capacity, assigned));
            assigned_total = assigned_total.saturating_add(assigned);
        }

        let mut remainder = network.total_milliunits.saturating_sub(assigned_total);
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
