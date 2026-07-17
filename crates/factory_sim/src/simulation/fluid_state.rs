use super::*;
use crate::simulation::fluid_ops::{FluidBoxAssignment, FluidBoxKey, FluidNetworkTopology};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct FluidSubsystem {
    pub(super) networks: Vec<FluidNetworkSnapshot>,
    #[serde(skip, default = "default_topology_dirty")]
    pub(super) topology_dirty: bool,
    #[serde(skip, default)]
    pub(super) topology_networks: Vec<FluidNetworkTopology>,
    #[serde(skip, default)]
    pub(super) network_ids_by_box: HashMap<FluidBoxKey, u32>,
    /// Networks whose box contents changed since their last redistribution.
    #[serde(skip, default)]
    pub(super) networks_needing_equalization: Vec<bool>,
    /// Networks whose durable snapshots no longer match their box contents.
    #[serde(skip, default)]
    pub(super) networks_needing_snapshot: Vec<bool>,
    /// Retained across networks and ticks to make redistribution allocation-free.
    #[serde(skip, default)]
    pub(super) equalization_assignments: Vec<FluidBoxAssignment>,
    #[cfg(test)]
    #[serde(skip, default)]
    pub(super) topology_rebuilds: u64,
}

impl Default for FluidSubsystem {
    fn default() -> Self {
        Self {
            networks: Vec::new(),
            topology_dirty: true,
            topology_networks: Vec::new(),
            network_ids_by_box: HashMap::new(),
            networks_needing_equalization: Vec::new(),
            networks_needing_snapshot: Vec::new(),
            equalization_assignments: Vec::new(),
            #[cfg(test)]
            topology_rebuilds: 0,
        }
    }
}

impl FluidSubsystem {
    pub(super) fn from_networks(networks: Vec<FluidNetworkSnapshot>) -> Self {
        Self {
            networks,
            topology_dirty: true,
            topology_networks: Vec::new(),
            network_ids_by_box: HashMap::new(),
            networks_needing_equalization: Vec::new(),
            networks_needing_snapshot: Vec::new(),
            equalization_assignments: Vec::new(),
            #[cfg(test)]
            topology_rebuilds: 0,
        }
    }

    pub(super) fn clear_networks(&mut self) {
        self.networks.clear();
        self.clear_topology();
    }

    pub(super) fn replace_topology(&mut self, topology_networks: Vec<FluidNetworkTopology>) {
        self.network_ids_by_box = network_ids_by_box(&topology_networks);
        self.topology_networks = topology_networks;
        self.networks_needing_equalization.clear();
        self.networks_needing_equalization
            .resize(self.topology_networks.len(), true);
        self.networks_needing_snapshot.clear();
        self.networks_needing_snapshot
            .resize(self.topology_networks.len(), true);
        self.topology_dirty = false;
    }

    pub(super) fn mark_network_dirty(&mut self, network_id: u32) {
        let network_index = network_id as usize;
        debug_assert_eq!(
            self.topology_networks
                .get(network_index)
                .map(|network| network.network_id),
            Some(network_id),
            "fluid network ids must remain dense and index-addressable"
        );
        if let Some(needs_equalization) = self.networks_needing_equalization.get_mut(network_index)
        {
            *needs_equalization = true;
        }
        if let Some(needs_snapshot) = self.networks_needing_snapshot.get_mut(network_index) {
            *needs_snapshot = true;
        }
    }

    pub(super) fn mark_box_dirty(&mut self, key: FluidBoxKey) {
        if let Some(network_id) = self.network_ids_by_box.get(&key).copied() {
            self.mark_network_dirty(network_id);
        }
    }

    fn clear_topology(&mut self) {
        self.topology_dirty = true;
        self.topology_networks.clear();
        self.network_ids_by_box.clear();
        self.networks_needing_equalization.clear();
        self.networks_needing_snapshot.clear();
    }
}

impl Hash for FluidSubsystem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.networks.hash(state);
    }
}

impl PartialEq for FluidSubsystem {
    fn eq(&self, other: &Self) -> bool {
        self.networks == other.networks
    }
}

fn default_topology_dirty() -> bool {
    true
}

fn network_ids_by_box(networks: &[FluidNetworkTopology]) -> HashMap<FluidBoxKey, u32> {
    let box_count = networks.iter().map(|network| network.boxes.len()).sum();
    let mut network_ids_by_box = HashMap::with_capacity(box_count);
    for network in networks {
        for box_topology in &network.boxes {
            network_ids_by_box.insert(box_topology.key, network.network_id);
        }
    }
    network_ids_by_box
}
