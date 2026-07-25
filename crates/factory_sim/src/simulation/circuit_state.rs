use super::*;
use std::collections::BTreeMap;

/// Which signal network each wired connector belongs to, per wire color.
///
/// Network ids are dense and assigned in ascending order of each component's
/// minimum [`CircuitNode`], so the same wiring always produces the same ids
/// regardless of how the entity maps happen to be iterated.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct CircuitTopology {
    pub(in crate::simulation) network_ids: BTreeMap<(CircuitNode, WireColor), u32>,
    pub(in crate::simulation) network_count: usize,
}

impl CircuitTopology {
    pub(in crate::simulation) fn network_id(
        &self,
        node: CircuitNode,
        color: WireColor,
    ) -> Option<u32> {
        self.network_ids.get(&(node, color)).copied()
    }
}

/// Runtime circuit state. Everything here is derived from the durable wire
/// connections and combinator configuration in [`EntityStore`], so it is
/// rebuilt on load rather than serialized.
#[derive(Clone, Debug, Default)]
pub(in crate::simulation) struct CircuitSubsystem {
    pub(in crate::simulation) topology_dirty: bool,
    pub(in crate::simulation) topology: CircuitTopology,
    /// Merged signal values per network id, indexed by id.
    pub(in crate::simulation) networks: Vec<SignalSet>,
    /// Combinator results computed this tick, published on the next one.
    pub(in crate::simulation) pending_outputs: Vec<(EntityId, Vec<(SignalId, i32)>)>,
    /// Reused between combinators so evaluation does not allocate per entity.
    pub(in crate::simulation) evaluation_scratch: SignalSet,
    /// Entities whose enable condition failed this tick, sorted by id.
    ///
    /// Resolved once per tick so consumers spread across the tick agree on one
    /// answer, and so belt advancement — which runs over a dense lane graph
    /// with the entity store mutably borrowed — can consult it without
    /// re-entering the simulation.
    pub(in crate::simulation) disabled_entities: Vec<EntityId>,
    #[cfg(test)]
    pub(in crate::simulation) topology_rebuilds: u64,
}

impl CircuitSubsystem {
    pub(in crate::simulation) fn invalidate_topology(&mut self) {
        self.topology_dirty = true;
        self.topology = CircuitTopology::default();
        self.networks.clear();
    }

    /// Merged value of `signal` as seen from `node`, summing the red and green
    /// networks it is wired to.
    pub(in crate::simulation) fn value_at(&self, node: CircuitNode, signal: SignalId) -> i32 {
        WireColor::ALL
            .into_iter()
            .filter_map(|color| self.network_at(node, color))
            .fold(0, |total, network| {
                total.wrapping_add(network.value(signal))
            })
    }

    /// Every signal reaching `node`, with red and green merged. Returns an
    /// owned set because the merge has no single backing network.
    pub(in crate::simulation) fn merged_at(&self, node: CircuitNode, out: &mut SignalSet) {
        out.clear();
        for color in WireColor::ALL {
            if let Some(network) = self.network_at(node, color) {
                out.extend_from(network);
            }
        }
    }

    pub(in crate::simulation) fn is_disabled(&self, entity_id: EntityId) -> bool {
        !self.disabled_entities.is_empty()
            && self.disabled_entities.binary_search(&entity_id).is_ok()
    }

    pub(in crate::simulation) fn is_wired(&self, node: CircuitNode) -> bool {
        WireColor::ALL
            .into_iter()
            .any(|color| self.topology.network_id(node, color).is_some())
    }

    fn network_at(&self, node: CircuitNode, color: WireColor) -> Option<&SignalSet> {
        let network_id = self.topology.network_id(node, color)?;
        self.networks.get(network_id as usize)
    }
}

// Circuit networks are a cache over durable wire connections and combinator
// configuration, so they take no part in simulation identity.
impl_runtime_only_identity!(CircuitSubsystem);
