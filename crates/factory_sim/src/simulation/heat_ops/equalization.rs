use crate::simulation::edge_geometry::rotated_edge_endpoint;
use crate::simulation::*;

use super::network_access::update_heat_network_snapshot;
use super::network_builder::build_heat_network_topology_from_nodes;
use super::types::{HeatBufferNode, HeatNetworkTopology};

impl Simulation {
    pub(in crate::simulation) fn ensure_heat_network_topology(&mut self) {
        if !self.heat.topology_dirty {
            return;
        }

        let topology_networks = self.build_heat_network_topology();
        self.heat.replace_topology(topology_networks);
        #[cfg(test)]
        {
            self.heat.topology_rebuilds += 1;
        }
    }

    /// Settles every dirty network to a common temperature.
    pub(in crate::simulation) fn equalize_heat_networks(&mut self) {
        self.ensure_heat_network_topology();
        for network_index in 0..self.heat.topology_networks.len() {
            if !self.heat.networks_needing_solve[network_index] {
                continue;
            }
            self.heat.networks_needing_solve[network_index] = false;
            self.heat.networks_needing_snapshot[network_index] = true;
            equalize_heat_network(
                &mut self.entities,
                &self.heat.topology_networks[network_index],
            );
        }
    }

    pub(in crate::simulation) fn refresh_heat_network_snapshots(&mut self) {
        self.ensure_heat_network_topology();
        self.heat.networks.resize(
            self.heat.topology_networks.len(),
            HeatNetworkSnapshot::default(),
        );
        for network_index in 0..self.heat.topology_networks.len() {
            if !self.heat.networks_needing_snapshot[network_index] {
                continue;
            }
            self.heat.networks_needing_snapshot[network_index] = false;
            update_heat_network_snapshot(
                &self.entities,
                &self.heat.topology_networks[network_index],
                &mut self.heat.networks[network_index],
            );
        }
    }

    fn build_heat_network_topology(&self) -> Vec<HeatNetworkTopology> {
        let nodes = self.heat_buffer_nodes();
        build_heat_network_topology_from_nodes(&nodes)
    }

    fn heat_buffer_nodes(&self) -> Vec<HeatBufferNode> {
        let mut nodes = Vec::new();
        for placed in self.entities.placed_entities.values() {
            if !self.entities.heat_buffers.contains_key(&placed.id) {
                continue;
            }
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(heat_buffer) = prototype.heat_buffer.as_ref() else {
                continue;
            };

            let endpoints = heat_buffer
                .connections
                .iter()
                .filter_map(|connection| rotated_edge_endpoint(placed, prototype, connection))
                .collect();
            nodes.push(HeatBufferNode {
                entity_id: placed.id,
                specific_heat_joules_per_degree: heat_buffer.specific_heat_joules_per_degree,
                max_temperature_degrees: heat_buffer.max_temperature_degrees,
                endpoints,
            });
        }
        nodes
    }
}

/// Redistributes a network's stored energy so every buffer settles at the same
/// temperature, clamping any buffer that would exceed its own maximum.
///
/// Equal temperature means each buffer holds energy proportional to its specific
/// heat. Buffers are visited in ascending maximum-temperature order (fixed when
/// the topology was built), so the first buffer whose limit the settling
/// temperature respects also settles every buffer after it: one pass, no sorting,
/// no allocation. Total energy is preserved exactly — the truncated remainder of
/// the proportional split is handed out one joule at a time to buffers with
/// headroom.
fn equalize_heat_network(entities: &mut EntityStore, network: &HeatNetworkTopology) {
    if network.buffers.is_empty() || network.specific_heat_joules_per_degree == 0 {
        return;
    }

    let mut remaining_energy = network
        .buffers
        .iter()
        .filter_map(|buffer| entities.heat_buffers.get(&buffer.entity_id))
        .fold(0_u64, |total, state| {
            total.saturating_add(state.energy_joules)
        });
    let mut remaining_specific_heat = network.specific_heat_joules_per_degree;

    for (buffer_index, buffer) in network.buffers.iter().enumerate() {
        if remaining_specific_heat == 0 {
            break;
        }
        // Would settling here overshoot this buffer's maximum temperature? Compare
        // energy-per-specific-heat by cross-multiplication so the decision never
        // depends on integer division rounding.
        let overshoots_maximum = u128::from(remaining_energy)
            * u128::from(buffer.specific_heat_joules_per_degree)
            > u128::from(buffer.capacity_joules) * u128::from(remaining_specific_heat);
        if overshoots_maximum {
            set_buffer_energy(entities, buffer.entity_id, buffer.capacity_joules);
            remaining_energy = remaining_energy.saturating_sub(buffer.capacity_joules);
            remaining_specific_heat =
                remaining_specific_heat.saturating_sub(buffer.specific_heat_joules_per_degree);
            continue;
        }

        distribute_energy_proportionally(
            entities,
            &network.buffers[buffer_index..],
            remaining_energy,
            remaining_specific_heat,
        );
        return;
    }

    // Every buffer clamped to its maximum: the network is saturated. Producers
    // refuse to add energy a buffer cannot hold, so any leftover here would mean
    // energy was injected past capacity.
    debug_assert_eq!(
        remaining_energy, 0,
        "a saturated heat network must not hold energy beyond its capacity"
    );
}

fn distribute_energy_proportionally(
    entities: &mut EntityStore,
    buffers: &[super::types::HeatNetworkBufferTopology],
    energy_joules: u64,
    specific_heat_joules_per_degree: u64,
) {
    debug_assert!(specific_heat_joules_per_degree > 0);
    let mut assigned_total = 0_u64;
    for buffer in buffers {
        let assigned = ((u128::from(energy_joules)
            * u128::from(buffer.specific_heat_joules_per_degree))
            / u128::from(specific_heat_joules_per_degree)) as u64;
        let assigned = assigned.min(buffer.capacity_joules);
        set_buffer_energy(entities, buffer.entity_id, assigned);
        assigned_total = assigned_total.saturating_add(assigned);
    }

    // Hand the truncated remainder out one joule at a time so the network total
    // is preserved exactly. At most one joule per buffer is needed, because each
    // buffer lost strictly less than one joule to truncation.
    let mut remainder = energy_joules.saturating_sub(assigned_total);
    for buffer in buffers {
        if remainder == 0 {
            break;
        }
        let Some(state) = entities.heat_buffers.get_mut(&buffer.entity_id) else {
            continue;
        };
        if state.energy_joules < buffer.capacity_joules {
            state.energy_joules += 1;
            remainder -= 1;
        }
    }
    debug_assert_eq!(
        remainder, 0,
        "the proportional split must leave room for its own remainder"
    );
}

fn set_buffer_energy(entities: &mut EntityStore, entity_id: EntityId, energy_joules: u64) {
    if let Some(state) = entities.heat_buffers.get_mut(&entity_id) {
        state.energy_joules = energy_joules;
    }
}
