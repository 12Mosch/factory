use super::*;
use std::hash::{Hash, Hasher};

/// Runtime-only consumer demand index.
///
/// Demand is durable through `entity_statuses`; this index only avoids
/// rediscovering and re-aggregating unchanged demand every tick. It is rebuilt
/// after loading or a power topology change and deliberately does not
/// participate in simulation identity.
#[derive(Clone, Debug, Default)]
pub(super) struct PowerDemandCache {
    pub(super) valid: bool,
    pub(super) active_consumers: Vec<EntityId>,
    pub(super) dirty_consumers: Vec<EntityId>,
    pub(super) refresh_consumers: Vec<EntityId>,
    pub(super) network_consumption_watts: Vec<u64>,
    pub(super) network_consumer_counts: Vec<usize>,
    pub(super) consumers_by_network: Vec<Vec<EntityId>>,
    pub(super) network_satisfaction_permyriad: Vec<u32>,
    #[cfg(test)]
    pub(super) demand_recomputations: u64,
}

impl PowerDemandCache {
    pub(super) fn invalidate(&mut self) {
        self.valid = false;
        self.dirty_consumers.clear();
    }

    pub(super) fn mark_dirty(&mut self, entity_id: EntityId) {
        if self.valid {
            self.dirty_consumers.push(entity_id);
        }
    }
}

impl PartialEq for PowerDemandCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Hash for PowerDemandCache {
    fn hash<H: Hasher>(&self, _state: &mut H) {}
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub(super) struct PowerSubsystem {
    pub(super) summary: PowerSummary,
    pub(super) networks: Vec<PowerNetworkSnapshot>,
    pub(super) entity_statuses: DenseEntityMap<EntityPowerStatus>,
    pub(super) topology_dirty: bool,
    pub(super) topology: PowerTopologyCache,
    #[cfg(test)]
    pub(super) topology_rebuilds: u64,
}

impl Default for PowerSubsystem {
    fn default() -> Self {
        Self {
            summary: PowerSummary {
                satisfaction_permyriad: POWER_SATISFACTION_FULL_PERMYRIAD,
                ..PowerSummary::default()
            },
            networks: Vec::new(),
            entity_statuses: DenseEntityMap::default(),
            topology_dirty: true,
            topology: PowerTopologyCache::default(),
            #[cfg(test)]
            topology_rebuilds: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub(super) struct PowerTopologyCache {
    pub(super) network_ids_by_entity: BTreeMap<EntityId, u32>,
    pub(super) pole_counts: Vec<usize>,
}
