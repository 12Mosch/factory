use std::collections::BTreeMap;

use crate::ids::EntityId;
use crate::simulation::disjoint_set::DisjointSet;
use crate::simulation::edge_geometry::EdgeEndpoint;

use super::types::{HeatBufferNode, HeatNetworkBufferTopology, HeatNetworkTopology};

/// Groups heat buffers into networks by shared footprint edges.
///
/// Two buffers join when their openings resolve to the same tile edge, the same
/// rule fluid boxes use. Networks are numbered by their lowest entity id so the
/// numbering is a deterministic function of the world, not of iteration order.
pub(super) fn build_heat_network_topology_from_nodes(
    nodes: &[HeatBufferNode],
) -> Vec<HeatNetworkTopology> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut disjoint_set = DisjointSet::new(nodes.len());
    let mut endpoint_buffers = BTreeMap::<EdgeEndpoint, Vec<usize>>::new();
    for (index, node) in nodes.iter().enumerate() {
        for endpoint in &node.endpoints {
            endpoint_buffers.entry(*endpoint).or_default().push(index);
        }
    }

    for indices in endpoint_buffers.values() {
        let Some((&first, rest)) = indices.split_first() else {
            continue;
        };
        for index in rest {
            disjoint_set.union(first, *index);
        }
    }

    let mut components_by_min_entity = BTreeMap::<EntityId, Vec<usize>>::new();
    for indices in disjoint_set.components().into_values() {
        let min_entity_id = indices
            .iter()
            .map(|index| nodes[*index].entity_id)
            .min()
            .expect("component should contain at least one heat buffer");
        components_by_min_entity.insert(min_entity_id, indices);
    }

    components_by_min_entity
        .into_values()
        .enumerate()
        .map(|(network_id, mut indices)| {
            // Ascending maximum temperature is what lets the solve fill buffers
            // in a single pass; entity id breaks ties deterministically.
            indices.sort_by_key(|index| {
                (
                    nodes[*index].max_temperature_degrees,
                    nodes[*index].entity_id,
                )
            });
            heat_network_topology(network_id as u32, nodes, &indices)
        })
        .collect()
}

fn heat_network_topology(
    network_id: u32,
    nodes: &[HeatBufferNode],
    indices: &[usize],
) -> HeatNetworkTopology {
    let mut buffers = Vec::with_capacity(indices.len());
    let mut specific_heat_joules_per_degree = 0_u64;
    let mut capacity_joules = 0_u64;

    for index in indices {
        let node = &nodes[*index];
        let capacity = node
            .specific_heat_joules_per_degree
            .saturating_mul(u64::from(
                node.max_temperature_degrees
                    .saturating_sub(factory_data::HEAT_AMBIENT_TEMPERATURE_DEGREES),
            ));
        buffers.push(HeatNetworkBufferTopology {
            entity_id: node.entity_id,
            specific_heat_joules_per_degree: node.specific_heat_joules_per_degree,
            capacity_joules: capacity,
        });
        specific_heat_joules_per_degree =
            specific_heat_joules_per_degree.saturating_add(node.specific_heat_joules_per_degree);
        capacity_joules = capacity_joules.saturating_add(capacity);
    }

    HeatNetworkTopology {
        network_id,
        buffers,
        specific_heat_joules_per_degree,
        capacity_joules,
    }
}
