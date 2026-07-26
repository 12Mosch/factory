use std::collections::BTreeMap;

use crate::ids::EntityId;
use crate::robots::TileBounds;
use crate::simulation::disjoint_set::DisjointSet;

use super::types::{RoboportNode, RobotNetworkTopology};

/// Groups roboports into networks by overlapping logistic areas.
///
/// Networks are numbered by their lowest entity id, so the numbering is a
/// deterministic function of the world rather than of iteration order — the
/// same rule power networks use, and the reason a rebuild after an unrelated
/// placement does not silently renumber the networks a player is looking at.
pub(super) fn build_robot_network_topology_from_nodes(
    nodes: &[RoboportNode],
) -> Vec<RobotNetworkTopology> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut disjoint_set = DisjointSet::new(nodes.len());
    connect_overlapping_logistic_areas(nodes, &mut disjoint_set);

    let mut components_by_min_entity = BTreeMap::<EntityId, Vec<usize>>::new();
    for indices in disjoint_set.components().into_values() {
        let min_entity_id = indices
            .iter()
            .map(|index| nodes[*index].entity_id)
            .min()
            .expect("component should contain at least one roboport");
        components_by_min_entity.insert(min_entity_id, indices);
    }

    components_by_min_entity
        .into_values()
        .enumerate()
        .map(|(network_id, mut indices)| {
            indices.sort_by_key(|index| nodes[*index].entity_id);
            robot_network_topology(network_id as u32, nodes, &indices)
        })
        .collect()
}

/// Unions every pair of roboports whose logistic squares overlap.
///
/// Roboports are bucketed by their logistic square's lower-left corner so only
/// nearby pairs are compared. Two overlapping squares have corners at most
/// `max_span - 1` tiles apart on each axis, so bucketing by `max_span` puts an
/// overlapping partner in the same bucket or one of its eight neighbours — the
/// scan below is therefore exhaustive, not a heuristic.
fn connect_overlapping_logistic_areas(nodes: &[RoboportNode], disjoint_set: &mut DisjointSet) {
    let max_span = nodes
        .iter()
        .map(|node| logistic_span(node.logistic_bounds))
        .max()
        .unwrap_or(1)
        .max(1);
    let mut buckets = BTreeMap::<(i64, i64), Vec<usize>>::new();
    for (index, node) in nodes.iter().enumerate() {
        buckets
            .entry(bucket_of(node, max_span))
            .or_default()
            .push(index);
    }

    for (index, node) in nodes.iter().enumerate() {
        let (bucket_x, bucket_y) = bucket_of(node, max_span);
        for y in bucket_y - 1..=bucket_y + 1 {
            for x in bucket_x - 1..=bucket_x + 1 {
                let Some(candidate_indices) = buckets.get(&(x, y)) else {
                    continue;
                };
                for candidate_index in candidate_indices {
                    if *candidate_index <= index {
                        continue;
                    }
                    if node
                        .logistic_bounds
                        .intersects(nodes[*candidate_index].logistic_bounds)
                    {
                        disjoint_set.union(index, *candidate_index);
                    }
                }
            }
        }
    }
}

/// Width of a logistic square in tiles, saturating so a degenerate rectangle
/// can never produce a zero or negative bucket span.
fn logistic_span(bounds: TileBounds) -> i64 {
    bounds
        .max_x
        .saturating_sub(bounds.min_x)
        .max(bounds.max_y.saturating_sub(bounds.min_y))
        .saturating_add(1)
        .max(1)
}

fn bucket_of(node: &RoboportNode, span: i64) -> (i64, i64) {
    (
        node.logistic_bounds.min_x.div_euclid(span),
        node.logistic_bounds.min_y.div_euclid(span),
    )
}

fn robot_network_topology(
    network_id: u32,
    nodes: &[RoboportNode],
    indices: &[usize],
) -> RobotNetworkTopology {
    let mut roboports = Vec::with_capacity(indices.len());
    let mut construction_bounds = None;
    let mut logistic_bounds = None;
    let mut charge_capacity_joules = 0_u64;

    for index in indices {
        let node = nodes[*index];
        roboports.push(node);
        construction_bounds = Some(match construction_bounds {
            Some(bounds) => TileBounds::union(bounds, node.construction_bounds),
            None => node.construction_bounds,
        });
        logistic_bounds = Some(match logistic_bounds {
            Some(bounds) => TileBounds::union(bounds, node.logistic_bounds),
            None => node.logistic_bounds,
        });
        charge_capacity_joules = charge_capacity_joules.saturating_add(node.charge_capacity_joules);
    }

    RobotNetworkTopology {
        network_id,
        roboports,
        construction_bounds: construction_bounds.unwrap_or_default(),
        logistic_bounds: logistic_bounds.unwrap_or_default(),
        charge_capacity_joules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(entity_id: u64, center_x: i64, center_y: i64, logistic_radius: i64) -> RoboportNode {
        RoboportNode {
            entity_id: EntityId::new(entity_id),
            construction_bounds: square(center_x, center_y, logistic_radius * 2),
            logistic_bounds: square(center_x, center_y, logistic_radius),
            charge_capacity_joules: 1_000,
        }
    }

    fn square(center_x: i64, center_y: i64, radius: i64) -> TileBounds {
        TileBounds {
            min_x: center_x - radius,
            min_y: center_y - radius,
            max_x: center_x + radius,
            max_y: center_y + radius,
        }
    }

    fn network_members(networks: &[RobotNetworkTopology]) -> Vec<Vec<u64>> {
        networks
            .iter()
            .map(|network| {
                network
                    .roboports
                    .iter()
                    .map(|roboport| roboport.entity_id.raw())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn overlapping_logistic_areas_form_one_network() {
        // Radius 10 squares 20 tiles apart still share their edge tiles.
        let nodes = [node(1, 0, 0, 10), node(2, 20, 0, 10)];

        let networks = build_robot_network_topology_from_nodes(&nodes);

        assert_eq!(network_members(&networks), vec![vec![1, 2]]);
    }

    #[test]
    fn separated_logistic_areas_form_separate_networks() {
        let nodes = [node(1, 0, 0, 10), node(2, 21, 0, 10)];

        let networks = build_robot_network_topology_from_nodes(&nodes);

        assert_eq!(network_members(&networks), vec![vec![1], vec![2]]);
    }

    /// The middle roboport bridges two that do not reach each other, which is
    /// the whole point of running the connectivity through a disjoint set.
    #[test]
    fn a_chain_merges_transitively() {
        let nodes = [node(1, 0, 0, 10), node(3, 40, 0, 10), node(2, 20, 0, 10)];

        let networks = build_robot_network_topology_from_nodes(&nodes);

        assert_eq!(network_members(&networks), vec![vec![1, 2, 3]]);
    }

    /// Numbering follows the lowest member entity id, not the order the nodes
    /// happened to arrive in.
    #[test]
    fn networks_are_numbered_by_lowest_member_entity_id() {
        let nodes = [node(7, 1_000, 0, 10), node(2, 0, 0, 10)];

        let networks = build_robot_network_topology_from_nodes(&nodes);

        assert_eq!(network_members(&networks), vec![vec![2], vec![7]]);
        assert_eq!(networks[0].network_id, 0);
        assert_eq!(networks[1].network_id, 1);
    }

    /// Bucketing must not miss a partner just because the two roboports have
    /// very different logistic radii.
    #[test]
    fn mismatched_radii_still_connect() {
        let nodes = [node(1, 0, 0, 2), node(2, 60, 0, 60)];

        let networks = build_robot_network_topology_from_nodes(&nodes);

        assert_eq!(network_members(&networks), vec![vec![1, 2]]);
    }

    #[test]
    fn network_bounds_union_the_member_squares() {
        let nodes = [node(1, 0, 0, 10), node(2, 20, 20, 10)];

        let networks = build_robot_network_topology_from_nodes(&nodes);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].logistic_bounds, square(10, 10, 20));
        assert_eq!(networks[0].charge_capacity_joules, 2_000);
    }
}
