use super::*;
use crate::simulation::heat_ops::HeatNetworkTopology;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Cached heat topology plus the durable network snapshots.
///
/// Laid out like [`crate::simulation::fluid_state::FluidSubsystem`] on purpose:
/// heat connectivity changes for exactly the same reasons fluid connectivity
/// does, so both subsystems share one invalidation story (`topology_dirty` for
/// placement changes, per-network dirty flags for the tick-local solve and the
/// snapshots the UI reads).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct HeatSubsystem {
    pub(super) networks: Vec<HeatNetworkSnapshot>,
    #[serde(skip, default = "default_topology_dirty")]
    pub(super) topology_dirty: bool,
    #[serde(skip, default)]
    pub(super) topology_networks: Vec<HeatNetworkTopology>,
    #[serde(skip, default)]
    pub(super) network_ids_by_entity: HashMap<EntityId, u32>,
    /// Networks whose buffer contents changed since their last solve.
    #[serde(skip, default)]
    pub(super) networks_needing_solve: Vec<bool>,
    /// Networks whose durable snapshots no longer match their buffer contents.
    #[serde(skip, default)]
    pub(super) networks_needing_snapshot: Vec<bool>,
    #[cfg(test)]
    #[serde(skip, default)]
    pub(super) topology_rebuilds: u64,
}

impl Default for HeatSubsystem {
    fn default() -> Self {
        Self {
            networks: Vec::new(),
            topology_dirty: true,
            topology_networks: Vec::new(),
            network_ids_by_entity: HashMap::new(),
            networks_needing_solve: Vec::new(),
            networks_needing_snapshot: Vec::new(),
            #[cfg(test)]
            topology_rebuilds: 0,
        }
    }
}

impl HeatSubsystem {
    pub(super) fn from_networks(networks: Vec<HeatNetworkSnapshot>) -> Self {
        Self {
            networks,
            ..Self::default()
        }
    }

    pub(super) fn clear_networks(&mut self) {
        self.networks.clear();
        self.topology_dirty = true;
        self.topology_networks.clear();
        self.network_ids_by_entity.clear();
        self.networks_needing_solve.clear();
        self.networks_needing_snapshot.clear();
    }

    pub(super) fn replace_topology(&mut self, topology_networks: Vec<HeatNetworkTopology>) {
        self.network_ids_by_entity = network_ids_by_entity(&topology_networks);
        self.topology_networks = topology_networks;
        self.networks_needing_solve.clear();
        self.networks_needing_solve
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
            "heat network ids must remain dense and index-addressable"
        );
        if let Some(needs_solve) = self.networks_needing_solve.get_mut(network_index) {
            *needs_solve = true;
        }
        if let Some(needs_snapshot) = self.networks_needing_snapshot.get_mut(network_index) {
            *needs_snapshot = true;
        }
    }

    pub(super) fn mark_buffer_dirty(&mut self, entity_id: EntityId) {
        if let Some(network_id) = self.network_ids_by_entity.get(&entity_id).copied() {
            self.mark_network_dirty(network_id);
        }
    }
}

// Only the durable snapshots participate in simulation identity; the topology
// cache is rebuilt from the entity store.
impl Hash for HeatSubsystem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.networks.hash(state);
    }
}

impl PartialEq for HeatSubsystem {
    fn eq(&self, other: &Self) -> bool {
        self.networks == other.networks
    }
}

fn default_topology_dirty() -> bool {
    true
}

fn network_ids_by_entity(networks: &[HeatNetworkTopology]) -> HashMap<EntityId, u32> {
    let buffer_count = networks.iter().map(|network| network.buffers.len()).sum();
    let mut network_ids_by_entity = HashMap::with_capacity(buffer_count);
    for network in networks {
        for buffer in &network.buffers {
            network_ids_by_entity.insert(buffer.entity_id, network.network_id);
        }
    }
    network_ids_by_entity
}
