use std::collections::BTreeMap;

use crate::simulation::disjoint_set::DisjointSet;

use super::types::{
    FluidBoxKey, FluidBoxNode, FluidEndpoint, FluidNetworkBoxTopology, FluidNetworkTopology,
};

pub(super) fn build_fluid_network_topology_from_nodes(
    nodes: &[FluidBoxNode],
) -> Vec<FluidNetworkTopology> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut disjoint_set = DisjointSet::new(nodes.len());
    let mut endpoint_boxes = BTreeMap::<FluidEndpoint, Vec<usize>>::new();
    for (index, node) in nodes.iter().enumerate() {
        for endpoint in &node.endpoints {
            endpoint_boxes.entry(*endpoint).or_default().push(index);
        }
    }

    for indices in endpoint_boxes.values() {
        let Some((&first, rest)) = indices.split_first() else {
            continue;
        };
        for index in rest {
            disjoint_set.union(first, *index);
        }
    }

    let mut components_by_min_key = BTreeMap::<FluidBoxKey, Vec<usize>>::new();
    for indices in disjoint_set.components().into_values() {
        let min_key = indices
            .iter()
            .map(|index| nodes[*index].key)
            .min()
            .expect("component should contain at least one fluid box");
        components_by_min_key.insert(min_key, indices);
    }

    components_by_min_key
        .into_values()
        .enumerate()
        .map(|(network_id, mut indices)| {
            indices.sort_by_key(|index| nodes[*index].key);
            fluid_network_topology(network_id as u32, nodes, &indices)
        })
        .collect()
}

fn fluid_network_topology(
    network_id: u32,
    nodes: &[FluidBoxNode],
    indices: &[usize],
) -> FluidNetworkTopology {
    let mut boxes = Vec::with_capacity(indices.len());
    let mut capacity_milliunits = 0_u64;

    for index in indices {
        let node = &nodes[*index];
        boxes.push(FluidNetworkBoxTopology {
            key: node.key,
            capacity_milliunits: node.capacity_milliunits,
            filter: node.filter,
        });
        capacity_milliunits = capacity_milliunits.saturating_add(node.capacity_milliunits);
    }

    FluidNetworkTopology {
        network_id,
        boxes,
        capacity_milliunits,
    }
}
