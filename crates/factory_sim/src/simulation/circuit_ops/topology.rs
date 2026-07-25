use super::*;
use crate::simulation::disjoint_set::DisjointSet;

impl Simulation {
    pub(in crate::simulation) fn invalidate_circuit_topology(&mut self) {
        self.circuits.invalidate_topology();
    }

    pub(in crate::simulation) fn ensure_circuit_topology(&mut self) {
        if !self.circuits.topology_dirty {
            return;
        }
        self.circuits.topology = rebuild_circuit_topology(&self.entities);
        self.circuits.topology_dirty = false;
        self.circuits
            .networks
            .resize_with(self.circuits.topology.network_count, SignalSet::default);
        #[cfg(test)]
        {
            self.circuits.topology_rebuilds += 1;
        }
    }

    #[cfg(test)]
    pub(in crate::simulation) fn circuit_topology_rebuild_count(&self) -> u64 {
        self.circuits.topology_rebuilds
    }
}

/// Groups wired connectors into networks, one independent pass per wire color.
///
/// Both endpoints of a wire are recorded on both entities, so walking every
/// entity's own links visits each wire twice; the disjoint set makes that
/// harmless and saves keeping a separate edge list.
pub(in crate::simulation) fn rebuild_circuit_topology(entities: &EntityStore) -> CircuitTopology {
    let mut topology = CircuitTopology::default();
    let mut next_network_id = 0_u32;

    for color in WireColor::ALL {
        let nodes = wired_nodes(entities, color);
        if nodes.is_empty() {
            continue;
        }

        let mut disjoint_set = DisjointSet::new(nodes.len());
        for (entity_id, state) in &entities.circuit_entities {
            for (port, link_color, neighbor) in state.connections.iter() {
                if link_color != color {
                    continue;
                }
                let local = CircuitNode::new(*entity_id, port);
                let (Ok(local_index), Ok(neighbor_index)) =
                    (nodes.binary_search(&local), nodes.binary_search(&neighbor))
                else {
                    continue;
                };
                disjoint_set.union(local_index, neighbor_index);
            }
        }

        // `nodes` is sorted, so the first index reaching a root is that
        // component's smallest node; assigning ids in index order therefore
        // orders networks by their minimum node.
        let mut network_ids_by_root = BTreeMap::<usize, u32>::new();
        for (index, node) in nodes.iter().enumerate() {
            let root = disjoint_set.find(index);
            let network_id = *network_ids_by_root.entry(root).or_insert_with(|| {
                let id = next_network_id;
                next_network_id += 1;
                id
            });
            topology.network_ids.insert((*node, color), network_id);
        }
    }

    topology.network_count = next_network_id as usize;
    topology
}

/// Every connector carrying at least one wire of `color`, sorted and deduped.
fn wired_nodes(entities: &EntityStore, color: WireColor) -> Vec<CircuitNode> {
    let mut nodes = Vec::new();
    for (entity_id, state) in &entities.circuit_entities {
        for (port, link_color, neighbor) in state.connections.iter() {
            if link_color != color {
                continue;
            }
            nodes.push(CircuitNode::new(*entity_id, port));
            nodes.push(neighbor);
        }
    }
    nodes.sort_unstable();
    nodes.dedup();
    nodes
}
