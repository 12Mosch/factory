use std::collections::BTreeMap;

use crate::ids::EntityId;
use crate::rail::{RailNetworkSnapshot, RailPoint};
use crate::simulation::SmallVec;
use crate::simulation::disjoint_set::DisjointSet;

use super::types::{RailEdge, RailGraph, RailNode, RailNodeEnd, RailPieceInput};

/// Builds the rail graph from the placed rails.
///
/// The fifth instance of the disjoint-set network solve the power, fluid, heat,
/// and robot networks already share. What differs is the shape of the input:
/// membership here is not symmetric proximity but a pair of rail ends that meet
/// at one point facing opposite ways, and each piece contributes an edge
/// carrying its own travel length rather than a bare membership.
///
/// Networks are numbered by their lowest member entity id, so numbering is a
/// deterministic function of the world rather than of iteration order.
pub(super) fn build_rail_graph_from_pieces(pieces: &[RailPieceInput]) -> RailGraph {
    let mut graph = RailGraph::default();
    if pieces.is_empty() {
        return graph;
    }

    let mut node_indices_by_position = BTreeMap::<RailPoint, usize>::new();
    for piece in pieces {
        let edge_index = graph.edges.len();
        let ends = piece.geometry.ends();
        let mut nodes = [0_usize; 2];
        for (end_index, end) in ends.iter().enumerate() {
            let node_index = *node_indices_by_position
                .entry(end.position)
                .or_insert_with(|| {
                    graph.nodes.push(RailNode {
                        ends: SmallVec::new(),
                    });
                    graph.nodes.len() - 1
                });
            graph.nodes[node_index].ends.push(RailNodeEnd {
                edge_index,
                end_index,
            });
            nodes[end_index] = node_index;
        }

        graph
            .edge_indices_by_entity
            .insert(piece.entity_id, edge_index);
        graph.edges.push(RailEdge {
            entity_id: piece.entity_id,
            nodes,
            headings: [ends[0].heading, ends[1].heading],
            length_fixed: piece.geometry.length_fixed,
            // Filled in once the components are known.
            network_id: 0,
        });
    }

    assign_networks(&mut graph);
    graph
}

/// Groups edges into connected networks and records the per-network totals.
fn assign_networks(graph: &mut RailGraph) {
    let mut disjoint_set = DisjointSet::new(graph.edges.len());
    for node in &graph.nodes {
        for (position, end) in node.ends.iter().enumerate() {
            for other in node.ends.iter().skip(position + 1) {
                let heading = graph.edges[end.edge_index].headings[end.end_index];
                let other_heading = graph.edges[other.edge_index].headings[other.end_index];
                if heading == other_heading.opposite() {
                    disjoint_set.union(end.edge_index, other.edge_index);
                }
            }
        }
    }

    let mut components_by_min_entity = BTreeMap::<EntityId, Vec<usize>>::new();
    for edge_indices in disjoint_set.components().into_values() {
        let min_entity_id = edge_indices
            .iter()
            .map(|index| graph.edges[*index].entity_id)
            .min()
            .expect("component should contain at least one rail piece");
        components_by_min_entity.insert(min_entity_id, edge_indices);
    }

    for (network_id, edge_indices) in components_by_min_entity.into_values().enumerate() {
        let network_id = network_id as u32;
        let mut total_length_fixed = 0_i64;
        let mut nodes = SmallVec::<[usize; 8]>::new();
        for edge_index in &edge_indices {
            let edge = &mut graph.edges[*edge_index];
            edge.network_id = network_id;
            total_length_fixed = total_length_fixed.saturating_add(edge.length_fixed);
            for node_index in edge.nodes {
                if !nodes.contains(&node_index) {
                    nodes.push(node_index);
                }
            }
        }

        graph.networks.push(RailNetworkSnapshot {
            network_id,
            piece_count: edge_indices.len(),
            node_count: nodes.len(),
            total_length_fixed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Direction;
    use crate::rail::{RailCurve, RailEnd, RailPieceGeometry};

    /// A two-tile straight piece running north from `(x, y)` in fixed point.
    fn straight(entity_id: u64, x: i64, y: i64) -> RailPieceInput {
        RailPieceInput {
            entity_id: EntityId::new(entity_id),
            geometry: RailPieceGeometry {
                start: RailEnd {
                    position: RailPoint::new(x, y),
                    heading: Direction::South,
                },
                end: RailEnd {
                    position: RailPoint::new(x, y + 2_048),
                    heading: Direction::North,
                },
                curve: RailCurve::Straight,
                length_fixed: 2_048,
            },
        }
    }

    /// Members of each network, sorted so a test states membership rather than
    /// the order the pieces happened to be handed to the builder.
    fn network_members(graph: &RailGraph) -> Vec<Vec<u64>> {
        graph
            .networks
            .iter()
            .map(|network| {
                let mut members = graph
                    .edges
                    .iter()
                    .filter(|edge| edge.network_id == network.network_id)
                    .map(|edge| edge.entity_id.raw())
                    .collect::<Vec<_>>();
                members.sort_unstable();
                members
            })
            .collect()
    }

    #[test]
    fn rails_meeting_end_to_end_form_one_network() {
        let graph = build_rail_graph_from_pieces(&[straight(1, 512, 0), straight(2, 512, 2_048)]);

        assert_eq!(network_members(&graph), vec![vec![1, 2]]);
        assert_eq!(graph.networks[0].piece_count, 2);
        // Two pieces sharing an end have three distinct ends between them.
        assert_eq!(graph.networks[0].node_count, 3);
        assert_eq!(graph.networks[0].total_length_fixed, 4_096);
    }

    /// Rails that only pass each other are two networks: connection is about
    /// ends meeting, not about footprints touching.
    #[test]
    fn rails_that_do_not_share_an_end_stay_separate() {
        let graph = build_rail_graph_from_pieces(&[straight(1, 512, 0), straight(2, 1_536, 0)]);

        assert_eq!(network_members(&graph), vec![vec![1], vec![2]]);
        assert_eq!(graph.networks.len(), 2);
    }

    /// The middle piece bridges two that do not touch each other, which is the
    /// whole reason connectivity runs through a disjoint set.
    #[test]
    fn a_run_merges_transitively() {
        let graph = build_rail_graph_from_pieces(&[
            straight(1, 512, 0),
            straight(3, 512, 4_096),
            straight(2, 512, 2_048),
        ]);

        assert_eq!(network_members(&graph), vec![vec![1, 2, 3]]);
        assert_eq!(graph.networks[0].node_count, 4);
    }

    #[test]
    fn networks_are_numbered_by_lowest_member_entity_id() {
        let graph = build_rail_graph_from_pieces(&[straight(7, 512, 0), straight(2, 8_192, 0)]);

        assert_eq!(network_members(&graph), vec![vec![2], vec![7]]);
        assert_eq!(graph.networks[0].network_id, 0);
        assert_eq!(graph.networks[1].network_id, 1);
    }

    /// Two ends at the same point facing the same way are two pieces laid over
    /// each other, not a junction, so they must not be joined. Placement refuses
    /// the pair; the graph must not invent a network out of it if one ever
    /// reaches it.
    #[test]
    fn ends_facing_the_same_way_do_not_connect() {
        let graph = build_rail_graph_from_pieces(&[straight(1, 512, 0), straight(2, 512, 0)]);

        assert_eq!(network_members(&graph), vec![vec![1], vec![2]]);
    }

    #[test]
    fn a_closed_loop_has_as_many_nodes_as_pieces() {
        // Four straights joined head to tail around a cycle: the last piece's
        // north end is the first piece's south end.
        let graph = build_rail_graph_from_pieces(&[
            straight(1, 512, 0),
            straight(2, 512, 2_048),
            straight(3, 512, 4_096),
            RailPieceInput {
                entity_id: EntityId::new(4),
                geometry: RailPieceGeometry {
                    start: RailEnd {
                        position: RailPoint::new(512, 6_144),
                        heading: Direction::South,
                    },
                    end: RailEnd {
                        position: RailPoint::new(512, 0),
                        heading: Direction::North,
                    },
                    curve: RailCurve::Straight,
                    length_fixed: 2_048,
                },
            },
        ]);

        assert_eq!(graph.networks.len(), 1);
        assert_eq!(graph.networks[0].piece_count, 4);
        assert_eq!(graph.networks[0].node_count, 4);
    }

    #[test]
    fn neighbors_resolve_to_the_rail_on_the_other_side_of_the_shared_end() {
        let graph = build_rail_graph_from_pieces(&[straight(1, 512, 0), straight(2, 512, 2_048)]);
        let first = graph
            .edge_for_entity(EntityId::new(1))
            .expect("piece one has an edge");

        assert_eq!(graph.neighbor(first, 0), None);
        assert_eq!(graph.neighbor(first, 1), Some(EntityId::new(2)));
    }
}
