use crate::heat::{HeatNetworkBufferSnapshot, temperature_millidegrees};
use crate::simulation::*;

use super::types::HeatNetworkTopology;

impl Simulation {
    pub(in crate::simulation) fn heat_network_id_for_entity(
        &self,
        entity_id: EntityId,
    ) -> Option<u32> {
        heat_network_id_for_entity(&self.heat, entity_id)
    }

    /// Adds heat to `entity_id`'s own buffer, returning the energy accepted.
    ///
    /// Producers add into their own buffer and let the network solve spread it,
    /// which is what keeps a reactor's output bounded by real thermal mass
    /// instead of appearing anywhere on the network at once.
    pub(in crate::simulation) fn add_heat_to_buffer(
        &mut self,
        entity_id: EntityId,
        energy_joules: u64,
    ) -> u64 {
        let capacity = self.heat_buffer_capacity_joules(entity_id);
        let Some(state) = self.entities.heat_buffers.get_mut(&entity_id) else {
            return 0;
        };
        let accepted = capacity
            .saturating_sub(state.energy_joules)
            .min(energy_joules);
        if accepted == 0 {
            return 0;
        }
        state.energy_joules += accepted;
        self.heat.mark_buffer_dirty(entity_id);
        accepted
    }

    /// Draws heat from `entity_id`'s own buffer, returning whether the full
    /// amount was available.
    pub(in crate::simulation) fn consume_heat_from_buffer(
        &mut self,
        entity_id: EntityId,
        energy_joules: u64,
    ) -> bool {
        let Some(state) = self.entities.heat_buffers.get_mut(&entity_id) else {
            return false;
        };
        if state.energy_joules < energy_joules {
            return false;
        }
        state.energy_joules -= energy_joules;
        self.heat.mark_buffer_dirty(entity_id);
        true
    }

    pub(in crate::simulation) fn heat_buffer_prototype(
        &self,
        entity_id: EntityId,
    ) -> Option<&factory_data::HeatBufferPrototype> {
        let placed = self.entities.placed_entity(entity_id)?;
        self.world
            .prototypes
            .entity(placed.prototype_id)?
            .heat_buffer
            .as_ref()
    }

    pub(in crate::simulation) fn heat_buffer_capacity_joules(&self, entity_id: EntityId) -> u64 {
        self.heat_buffer_prototype(entity_id)
            .map_or(0, factory_data::HeatBufferPrototype::capacity_joules)
    }

    /// Settled heat networks, one snapshot per network.
    pub fn heat_networks(&self) -> &[HeatNetworkSnapshot] {
        &self.heat.networks
    }

    /// Heat status of one entity, or `None` when it has no heat buffer.
    pub fn entity_heat_status(&self, entity_id: EntityId) -> Option<EntityHeatStatus> {
        let state = self.entities.heat_buffers.get(&entity_id)?;
        let heat_buffer = self.heat_buffer_prototype(entity_id)?;
        Some(EntityHeatStatus {
            network_id: self.heat_network_id_for_entity(entity_id),
            energy_joules: state.energy_joules,
            capacity_joules: heat_buffer.capacity_joules(),
            temperature_millidegrees: temperature_millidegrees(
                state.energy_joules,
                heat_buffer.specific_heat_joules_per_degree,
            ),
        })
    }

    #[cfg(test)]
    pub(in crate::simulation) fn heat_topology_rebuild_count(&self) -> u64 {
        self.heat.topology_rebuilds
    }
}

pub(in crate::simulation) fn heat_network_id_for_entity(
    heat: &HeatSubsystem,
    entity_id: EntityId,
) -> Option<u32> {
    debug_assert!(
        !heat.topology_dirty,
        "heat topology must be ensured before querying network ids"
    );
    heat.network_ids_by_entity.get(&entity_id).copied()
}

pub(super) fn update_heat_network_snapshot(
    entities: &EntityStore,
    network: &HeatNetworkTopology,
    snapshot: &mut HeatNetworkSnapshot,
) {
    snapshot.network_id = network.network_id;
    snapshot.buffer_count = network.buffers.len();
    snapshot.capacity_joules = network.capacity_joules;

    let mut energy_joules = 0_u64;
    let mut snapshot_index = 0;
    for buffer in &network.buffers {
        let Some(state) = entities.heat_buffers.get(&buffer.entity_id) else {
            continue;
        };
        energy_joules = energy_joules.saturating_add(state.energy_joules);
        let buffer_snapshot = HeatNetworkBufferSnapshot {
            entity_id: buffer.entity_id,
            energy_joules: state.energy_joules,
            capacity_joules: buffer.capacity_joules,
            temperature_millidegrees: temperature_millidegrees(
                state.energy_joules,
                buffer.specific_heat_joules_per_degree,
            ),
        };
        if let Some(existing) = snapshot.buffers.get_mut(snapshot_index) {
            *existing = buffer_snapshot;
        } else {
            snapshot.buffers.push(buffer_snapshot);
        }
        snapshot_index += 1;
    }
    snapshot.buffers.truncate(snapshot_index);
    snapshot.energy_joules = energy_joules;
    // The network is settled, so its temperature is the network-wide ratio of
    // stored energy to thermal mass.
    snapshot.temperature_millidegrees =
        temperature_millidegrees(energy_joules, network.specific_heat_joules_per_degree);
}
