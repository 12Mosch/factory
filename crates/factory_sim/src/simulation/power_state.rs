use super::*;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub(super) struct PowerSubsystem {
    pub(super) summary: PowerSummary,
    pub(super) networks: Vec<PowerNetworkSnapshot>,
    pub(super) entity_statuses: BTreeMap<EntityId, EntityPowerStatus>,
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
            entity_statuses: BTreeMap::new(),
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
