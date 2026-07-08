use std::collections::{BTreeMap, BTreeSet};

use crate::simulation::disjoint_set::DisjointSet;

use super::math::single_fluid;
use super::types::{BuiltFluidNetwork, FluidBoxKey, FluidBoxNode, FluidEndpoint};

pub(super) fn build_fluid_networks_from_nodes(nodes: &[FluidBoxNode]) -> Vec<BuiltFluidNetwork> {
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
            built_fluid_network(network_id as u32, nodes, &indices)
        })
        .collect()
}

fn built_fluid_network(
    network_id: u32,
    nodes: &[FluidBoxNode],
    indices: &[usize],
) -> BuiltFluidNetwork {
    let mut filters = BTreeSet::new();
    let mut nonempty_fluids = BTreeSet::new();
    let mut boxes = Vec::with_capacity(indices.len());
    let mut capacity_milliunits = 0_u64;
    let mut total_milliunits = 0_u64;

    for index in indices {
        let node = &nodes[*index];
        boxes.push(node.key);
        capacity_milliunits = capacity_milliunits.saturating_add(node.capacity_milliunits);
        total_milliunits = total_milliunits.saturating_add(node.amount_milliunits);
        if let Some(filter) = node.filter {
            filters.insert(filter);
        }
        if node.amount_milliunits > 0
            && let Some(fluid_id) = node.fluid_id
        {
            nonempty_fluids.insert(fluid_id);
        }
    }

    let filter_fluid = single_fluid(filters.iter().copied());
    let nonempty_fluid = single_fluid(nonempty_fluids.iter().copied());
    let blocked = filters.len() > 1
        || nonempty_fluids.len() > 1
        || filter_fluid
            .zip(nonempty_fluid)
            .is_some_and(|(filter, fluid)| filter != fluid);
    let fluid_id = if nonempty_fluids.len() > 1 {
        None
    } else {
        nonempty_fluid.or(filter_fluid)
    };

    BuiltFluidNetwork {
        network_id,
        boxes,
        capacity_milliunits,
        total_milliunits,
        fluid_id,
        blocked,
    }
}
