use super::super::*;
use crate::heat::temperature_millidegrees;

/// Every heat network entity must own exactly one heat buffer entry, and no other
/// entity may. Without this, a stale buffer would keep a phantom thermal mass on
/// the network after its entity changed kind.
pub(super) fn validate_heat_buffer_states(sim: &Simulation) -> Result<(), SimValidationError> {
    for placed in sim.entities.placed_entities.values() {
        let prototype = sim.world.prototypes.entity(placed.prototype_id).ok_or(
            SimValidationError::InvalidEntityPrototype {
                entity_id: placed.id,
                prototype_id: placed.prototype_id,
            },
        )?;
        let state = sim.entities.heat_buffers.get(&placed.id);
        match (prototype.heat_buffer.as_ref(), state) {
            (None, None) => {}
            (Some(heat_buffer), Some(state)) => {
                if state.energy_joules > heat_buffer.capacity_joules() {
                    return Err(SimValidationError::InvalidHeatBufferState {
                        entity_id: placed.id,
                    });
                }
            }
            _ => {
                return Err(SimValidationError::InvalidEntityState {
                    entity_id: placed.id,
                });
            }
        }
    }

    Ok(())
}

/// Checks the durable network snapshots against the buffers they summarize:
/// dense ids, every buffer in exactly one network, and totals plus the settled
/// temperature that follow from the buffer contents.
pub(super) fn validate_heat_network_snapshots(sim: &Simulation) -> Result<(), SimValidationError> {
    let expected_buffers = sim
        .entities
        .heat_buffers
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut networked_buffers = BTreeSet::new();

    for (expected_network_id, network) in sim.heat.networks.iter().enumerate() {
        let invalid = || SimValidationError::InvalidHeatNetwork {
            network_id: network.network_id,
        };
        if network.network_id != expected_network_id as u32
            || network.buffer_count != network.buffers.len()
            || network.energy_joules > network.capacity_joules
        {
            return Err(invalid());
        }

        let mut energy_joules = 0_u64;
        let mut capacity_joules = 0_u64;
        let mut specific_heat_joules_per_degree = 0_u64;
        for buffer in &network.buffers {
            if !networked_buffers.insert(buffer.entity_id) {
                return Err(invalid());
            }
            let placed = sim
                .entities
                .placed_entity(buffer.entity_id)
                .ok_or_else(invalid)?;
            let heat_buffer = sim
                .world
                .prototypes
                .entity(placed.prototype_id)
                .and_then(|prototype| prototype.heat_buffer.as_ref())
                .ok_or_else(invalid)?;
            let state = sim
                .entities
                .heat_buffers
                .get(&buffer.entity_id)
                .ok_or_else(invalid)?;

            if buffer.energy_joules != state.energy_joules
                || buffer.capacity_joules != heat_buffer.capacity_joules()
                || buffer.temperature_millidegrees
                    != temperature_millidegrees(
                        state.energy_joules,
                        heat_buffer.specific_heat_joules_per_degree,
                    )
            {
                return Err(invalid());
            }
            energy_joules = energy_joules.saturating_add(state.energy_joules);
            capacity_joules = capacity_joules.saturating_add(buffer.capacity_joules);
            specific_heat_joules_per_degree = specific_heat_joules_per_degree
                .saturating_add(heat_buffer.specific_heat_joules_per_degree);
        }

        if energy_joules != network.energy_joules
            || capacity_joules != network.capacity_joules
            || network.temperature_millidegrees
                != temperature_millidegrees(energy_joules, specific_heat_joules_per_degree)
        {
            return Err(invalid());
        }
    }

    if networked_buffers != expected_buffers {
        return Err(SimValidationError::InvalidHeatNetwork { network_id: 0 });
    }

    Ok(())
}
