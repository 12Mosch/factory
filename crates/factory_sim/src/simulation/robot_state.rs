use super::*;
use crate::simulation::robot_ops::RobotNetworkTopology;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Cached robot-network topology plus the durable network snapshots.
///
/// Laid out like [`crate::simulation::heat_state::HeatSubsystem`]: robot
/// connectivity changes for exactly the same reasons heat connectivity does
/// (something was placed or destroyed), so both share one invalidation story.
/// What robot networks do not have is a per-tick solve — nothing flows between
/// roboports — so there is a single `networks_needing_snapshot` flag set rather
/// than a separate solve pass.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RobotSubsystem {
    pub(super) networks: Vec<RobotNetworkSnapshot>,
    #[serde(skip, default = "default_topology_dirty")]
    pub(super) topology_dirty: bool,
    #[serde(skip, default)]
    pub(super) topology_networks: Vec<RobotNetworkTopology>,
    #[serde(skip, default)]
    pub(super) network_ids_by_entity: HashMap<EntityId, u32>,
    /// Networks whose durable snapshots no longer match their roboports.
    #[serde(skip, default)]
    pub(super) networks_needing_snapshot: Vec<bool>,
    #[cfg(test)]
    #[serde(skip, default)]
    pub(super) topology_rebuilds: u64,
}

impl Default for RobotSubsystem {
    fn default() -> Self {
        Self {
            networks: Vec::new(),
            topology_dirty: true,
            topology_networks: Vec::new(),
            network_ids_by_entity: HashMap::new(),
            networks_needing_snapshot: Vec::new(),
            #[cfg(test)]
            topology_rebuilds: 0,
        }
    }
}

impl RobotSubsystem {
    pub(super) fn from_networks(networks: Vec<RobotNetworkSnapshot>) -> Self {
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
        self.networks_needing_snapshot.clear();
    }

    pub(super) fn replace_topology(&mut self, topology_networks: Vec<RobotNetworkTopology>) {
        self.network_ids_by_entity = network_ids_by_entity(&topology_networks);
        self.topology_networks = topology_networks;
        self.networks_needing_snapshot.clear();
        self.networks_needing_snapshot
            .resize(self.topology_networks.len(), true);
        self.topology_dirty = false;
    }

    /// Marks the network owning `entity_id` as needing a fresh snapshot, which
    /// is how a changed charging buffer reaches the durable totals.
    pub(super) fn mark_roboport_dirty(&mut self, entity_id: EntityId) {
        let Some(network_id) = self.network_ids_by_entity.get(&entity_id).copied() else {
            return;
        };
        let network_index = network_id as usize;
        debug_assert_eq!(
            self.topology_networks
                .get(network_index)
                .map(|network| network.network_id),
            Some(network_id),
            "robot network ids must remain dense and index-addressable"
        );
        if let Some(needs_snapshot) = self.networks_needing_snapshot.get_mut(network_index) {
            *needs_snapshot = true;
        }
    }
}

// Only the durable snapshots participate in simulation identity; the topology
// cache is rebuilt from the entity store.
impl Hash for RobotSubsystem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.networks.hash(state);
    }
}

impl PartialEq for RobotSubsystem {
    fn eq(&self, other: &Self) -> bool {
        self.networks == other.networks
    }
}

fn default_topology_dirty() -> bool {
    true
}

fn network_ids_by_entity(networks: &[RobotNetworkTopology]) -> HashMap<EntityId, u32> {
    let roboport_count = networks
        .iter()
        .map(|network| network.roboports.len())
        .sum::<usize>();
    let mut network_ids_by_entity = HashMap::with_capacity(roboport_count);
    for network in networks {
        for roboport in &network.roboports {
            network_ids_by_entity.insert(roboport.entity_id, network.network_id);
        }
    }
    network_ids_by_entity
}
