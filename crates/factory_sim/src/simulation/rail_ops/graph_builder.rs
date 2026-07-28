use std::collections::BTreeMap;

use crate::ids::EntityId;
use crate::rails::RailPoint;
use crate::simulation::disjoint_set::DisjointSet;

use super::types::{
    RailEdgeNode, RailEndpointRef, RailGraph, RailGraphEdge, RailGraphNode, RailNetworkTopology,
};

/// Builds the rail graph from the placed pieces.
///
/// This is the fifth instance of the invalidate-and-rebuild connectivity solve
/// power, fluid, heat, and robot networks already share, and it reuses the same
/// [`DisjointSet`]. What differs is what an edge means: the other four ask only
/// whether two entities are members of one network, while a rail edge carries a
/// length and a heading at each end, because a train has to know how far it is
/// travelling and which way it is pointing when it gets there.
///
/// Two ends join only when they sit at the same point *and* face each other.
/// Ends that merely touch — a spur leaving north and another leaving east from
/// one point — are separate track, and keeping that rule here is what stops the
/// graph from inventing connections the geometry does not have.
///
/// `nodes` must be in ascending entity id order; the edges keep that order, and
/// [`RailGraph::edge_index`] finds a piece by binary search on it.
pub(super) fn build_rail_graph_from_nodes(nodes: &[RailEdgeNode]) -> RailGraph {
    debug_assert!(
        nodes
            .windows(2)
            .all(|pair| pair[0].entity_id < pair[1].entity_id),
        "rail edges must be built in ascending entity id order"
    );
    if nodes.is_empty() {
        return RailGraph::default();
    }

    let mut edges = Vec::with_capacity(nodes.len());
    let mut endpoints_by_position = BTreeMap::<RailPoint, Vec<RailEndpointRef>>::new();
    for (index, node) in nodes.iter().enumerate() {
        let edge = index as u32;
        for (endpoint, geometry) in node.geometry.endpoints.iter().enumerate() {
            endpoints_by_position
                .entry(geometry.position)
                .or_default()
                .push(RailEndpointRef {
                    edge,
                    endpoint: endpoint as u8,
                });
        }
        edges.push(RailGraphEdge {
            entity_id: node.entity_id,
            // Filled in below, once the junctions have their indices.
            nodes: [u32::MAX; 2],
            outward_directions: [
                node.geometry.endpoints[0].direction,
                node.geometry.endpoints[1].direction,
            ],
            length_fixed: node.geometry.length_fixed,
        });
    }

    let mut graph_nodes = Vec::with_capacity(endpoints_by_position.len());
    let mut disjoint_set = DisjointSet::new(nodes.len());
    for (position, endpoints) in endpoints_by_position {
        let node_index = graph_nodes.len() as u32;
        for reference in &endpoints {
            edges[reference.edge as usize].nodes[usize::from(reference.endpoint)] = node_index;
        }
        union_facing_endpoints(nodes, &endpoints, &mut disjoint_set);
        graph_nodes.push(RailGraphNode {
            position,
            endpoints,
        });
    }

    RailGraph {
        networks: build_networks(nodes, &edges, &mut disjoint_set),
        nodes: graph_nodes,
        edges,
    }
}

/// Joins every pair of ends at one junction that face each other.
///
/// A junction holds at most a handful of ends — tile-locked occupancy is what
/// bounds it — so the pairwise scan is cheap and exact rather than a heuristic.
fn union_facing_endpoints(
    nodes: &[RailEdgeNode],
    endpoints: &[RailEndpointRef],
    disjoint_set: &mut DisjointSet,
) {
    for (offset, first) in endpoints.iter().enumerate() {
        let first_endpoint = endpoint_geometry(nodes, *first);
        for second in &endpoints[offset + 1..] {
            if first.edge == second.edge {
                continue;
            }
            if first_endpoint.joins(endpoint_geometry(nodes, *second)) {
                disjoint_set.union(first.edge as usize, second.edge as usize);
            }
        }
    }
}

fn endpoint_geometry(
    nodes: &[RailEdgeNode],
    reference: RailEndpointRef,
) -> crate::rails::RailEndpoint {
    nodes[reference.edge as usize].geometry.endpoints[usize::from(reference.endpoint)]
}

/// Groups the connected edges into networks numbered by their lowest member
/// entity id, the rule every other network solve in the simulation uses.
fn build_networks(
    nodes: &[RailEdgeNode],
    edges: &[RailGraphEdge],
    disjoint_set: &mut DisjointSet,
) -> Vec<RailNetworkTopology> {
    let mut components_by_min_entity = BTreeMap::<EntityId, Vec<u32>>::new();
    for indices in disjoint_set.components().into_values() {
        let min_entity_id = indices
            .iter()
            .map(|index| nodes[*index].entity_id)
            .min()
            .expect("component should contain at least one rail piece");
        components_by_min_entity.insert(
            min_entity_id,
            indices.into_iter().map(|index| index as u32).collect(),
        );
    }

    components_by_min_entity
        .into_values()
        .enumerate()
        .map(|(network_id, mut member_edges)| {
            member_edges.sort_by_key(|edge| edges[*edge as usize].entity_id);
            let total_length_fixed = member_edges
                .iter()
                .map(|edge| u64::from(edges[*edge as usize].length_fixed))
                .sum();
            RailNetworkTopology {
                network_id: network_id as u32,
                edges: member_edges,
                total_length_fixed,
            }
        })
        .collect()
}
