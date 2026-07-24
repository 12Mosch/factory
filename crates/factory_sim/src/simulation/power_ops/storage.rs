use super::types::{AccumulatorEntry, NetworkPowerBalance};
use super::*;

impl Simulation {
    /// Sums connected solar generation into each network at the current
    /// daylight ratio. Solar is fuel-free, so it is collected before steam and
    /// contributes to `producer_count` whenever it produces power.
    pub(super) fn collect_solar_generation(
        &self,
        network_ids_by_entity: &BTreeMap<EntityId, u32>,
        networks: &mut [NetworkPowerBalance],
    ) {
        let (numerator, denominator) = self.daylight_ratio();
        for &entity_id in self.entities.solar_panels.keys() {
            let Some(network_id) = network_ids_by_entity.get(&entity_id).copied() else {
                continue;
            };
            let output = self.solar_panel_output_watts(entity_id, numerator, denominator);
            let Some(network) = networks.get_mut(network_id as usize) else {
                continue;
            };
            network.solar_watts = network.solar_watts.saturating_add(output);
            if output > 0 {
                network.producer_count += 1;
            }
        }
    }

    /// Deterministic integer solar output for one panel:
    /// `max_output × daylight_numerator / daylight_denominator`, flooring
    /// fractional watts. Missing prototypes produce no power.
    fn solar_panel_output_watts(
        &self,
        entity_id: EntityId,
        daylight_numerator: u64,
        daylight_denominator: u64,
    ) -> u64 {
        let output = self
            .entities
            .placed_entity(entity_id)
            .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
            .and_then(|prototype| prototype.solar_panel.as_ref())
            .map(|solar| {
                (u128::from(solar.max_power_output_watts) * u128::from(daylight_numerator)
                    / u128::from(daylight_denominator)) as u64
            });
        output.unwrap_or(0)
    }

    /// True when a connected solar panel currently produces power. Drives the
    /// electricity-generated onboarding milestone: storage discharge alone does
    /// not count as newly generated electricity.
    pub(super) fn any_solar_panel_can_generate(
        &self,
        network_ids_by_entity: &BTreeMap<EntityId, u32>,
    ) -> bool {
        let (numerator, denominator) = self.daylight_ratio();
        if numerator == 0 {
            return false;
        }
        self.entities.solar_panels.keys().any(|&entity_id| {
            network_ids_by_entity.contains_key(&entity_id)
                && self.solar_panel_output_watts(entity_id, numerator, denominator) > 0
        })
    }

    /// Collects per-accumulator charge/discharge capability for every network,
    /// grouping accumulators by network in ascending `EntityId` order so
    /// integer-leftover allocation stays deterministic.
    pub(super) fn collect_accumulator_capabilities(
        &self,
        network_ids_by_entity: &BTreeMap<EntityId, u32>,
        networks: &mut [NetworkPowerBalance],
        accumulators_by_network: &mut Vec<Vec<AccumulatorEntry>>,
    ) {
        accumulators_by_network.clear();
        accumulators_by_network.resize_with(networks.len(), Vec::new);
        for (&entity_id, state) in &self.entities.accumulators {
            let Some(network_id) = network_ids_by_entity.get(&entity_id).copied() else {
                continue;
            };
            let Some(prototype) = self
                .entities
                .placed_entity(entity_id)
                .and_then(|placed| self.world.prototypes.entity(placed.prototype_id))
                .and_then(|prototype| prototype.accumulator.as_ref())
            else {
                continue;
            };
            let Some(network) = networks.get_mut(network_id as usize) else {
                continue;
            };
            let stored_watt_ticks = stored_watt_ticks(state);
            let headroom_watt_ticks = u128::from(prototype.capacity_joules)
                * u128::from(SIMULATION_TICKS_PER_SECOND)
                - stored_watt_ticks;
            let charge_capability_watts =
                headroom_watt_ticks.min(u128::from(prototype.max_charge_watts)) as u64;
            let discharge_capability_watts =
                stored_watt_ticks.min(u128::from(prototype.max_discharge_watts)) as u64;

            network.accumulator_count += 1;
            network.charge_capability_watts = network
                .charge_capability_watts
                .saturating_add(charge_capability_watts);
            network.discharge_capability_watts = network
                .discharge_capability_watts
                .saturating_add(discharge_capability_watts);
            network.capacity_joules = network
                .capacity_joules
                .saturating_add(prototype.capacity_joules);
            accumulators_by_network[network_id as usize].push(AccumulatorEntry {
                entity_id,
                charge_capability_watts,
                discharge_capability_watts,
            });
        }
    }

    /// Distributes each network's allocated charge or discharge across its
    /// accumulators and folds the change into their durable stored energy.
    /// Reports the post-tick stored energy total back into the network so
    /// telemetry reflects the current state.
    pub(super) fn apply_accumulator_energy(
        &mut self,
        networks: &mut [NetworkPowerBalance],
        accumulators_by_network: &[Vec<AccumulatorEntry>],
        allocation_scratch: &mut Vec<u64>,
    ) {
        for (network_id, entries) in accumulators_by_network.iter().enumerate() {
            let network = &mut networks[network_id];
            if network.charge_watts > 0 {
                allocate_proportionally(
                    network.charge_watts,
                    entries,
                    allocation_scratch,
                    |entry| entry.charge_capability_watts,
                );
                for (entry, &watts) in entries.iter().zip(allocation_scratch.iter()) {
                    if watts > 0
                        && let Some(state) = self.entities.accumulators.get_mut(&entry.entity_id)
                    {
                        charge_accumulator(state, watts);
                    }
                }
            } else if network.discharge_watts > 0 {
                allocate_proportionally(
                    network.discharge_watts,
                    entries,
                    allocation_scratch,
                    |entry| entry.discharge_capability_watts,
                );
                for (entry, &watts) in entries.iter().zip(allocation_scratch.iter()) {
                    if watts > 0
                        && let Some(state) = self.entities.accumulators.get_mut(&entry.entity_id)
                    {
                        discharge_accumulator(state, watts);
                    }
                }
            }

            network.stored_energy_joules = 0;
            for entry in entries {
                let stored = self
                    .entities
                    .accumulators
                    .get(&entry.entity_id)
                    .map_or(0, |state| state.stored_energy_joules);
                network.stored_energy_joules = network.stored_energy_joules.saturating_add(stored);
            }
        }
    }
}

/// Total stored energy in watt-ticks (`stored_joules × ticks_per_second +
/// remainder`), the exact currency the charge/discharge math operates in.
fn stored_watt_ticks(state: &crate::power::AccumulatorState) -> u128 {
    u128::from(state.stored_energy_joules) * u128::from(SIMULATION_TICKS_PER_SECOND)
        + u128::from(state.energy_remainder_watt_ticks)
}

/// Adds `watts` watt-ticks of charge, normalizing back into whole joules plus a
/// sub-joule watt-tick remainder without any floating-point state.
fn charge_accumulator(state: &mut crate::power::AccumulatorState, watts: u64) {
    let total = stored_watt_ticks(state) + u128::from(watts);
    let ticks = u128::from(SIMULATION_TICKS_PER_SECOND);
    state.stored_energy_joules = (total / ticks) as u64;
    state.energy_remainder_watt_ticks = (total % ticks) as u8;
}

/// Removes `watts` watt-ticks of stored energy. Callers guarantee `watts` never
/// exceeds the accumulator's stored watt-ticks, so the subtraction never wraps.
fn discharge_accumulator(state: &mut crate::power::AccumulatorState, watts: u64) {
    let total = stored_watt_ticks(state) - u128::from(watts);
    let ticks = u128::from(SIMULATION_TICKS_PER_SECOND);
    state.stored_energy_joules = (total / ticks) as u64;
    state.energy_remainder_watt_ticks = (total % ticks) as u8;
}

/// Splits `total` watts across `entries` in proportion to each entry's
/// capability, then hands any integer leftover to accumulators in ascending
/// `EntityId` order (entries are pre-sorted). Callers guarantee
/// `total <= sum(capability)`, so every unit finds headroom.
fn allocate_proportionally(
    total: u64,
    entries: &[AccumulatorEntry],
    out: &mut Vec<u64>,
    capability_of: impl Fn(&AccumulatorEntry) -> u64,
) {
    out.clear();
    let total_capability: u128 = entries
        .iter()
        .map(|entry| u128::from(capability_of(entry)))
        .sum();
    if total_capability == 0 {
        out.resize(entries.len(), 0);
        return;
    }

    let total128 = u128::from(total);
    let mut assigned: u128 = 0;
    for entry in entries {
        let capability = u128::from(capability_of(entry));
        let share = (total128 * capability / total_capability) as u64;
        assigned += u128::from(share);
        out.push(share);
    }

    let mut leftover = total.saturating_sub(assigned as u64);
    for (index, entry) in entries.iter().enumerate() {
        if leftover == 0 {
            break;
        }
        if out[index] < capability_of(entry) {
            out[index] += 1;
            leftover -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::EntityId;
    use crate::power::AccumulatorState;

    fn entry(id: u64, charge: u64, discharge: u64) -> AccumulatorEntry {
        AccumulatorEntry {
            entity_id: EntityId::new(id),
            charge_capability_watts: charge,
            discharge_capability_watts: discharge,
        }
    }

    #[test]
    fn charge_and_discharge_conserve_exact_watt_ticks() {
        let mut state = AccumulatorState::default();
        // A per-tick amount that is not a whole number of joules exercises the
        // watt-tick remainder carry.
        for tick in 1..=7 {
            charge_accumulator(&mut state, 17_500);
            let total = stored_watt_ticks(&state);
            assert_eq!(total, u128::from(17_500u64 * tick));
            assert!(state.energy_remainder_watt_ticks < SIMULATION_TICKS_PER_SECOND as u8);
        }
        // 7 * 17_500 = 122_500 watt-ticks = 2041 J + 40 watt-ticks.
        assert_eq!(state.stored_energy_joules, 2_041);
        assert_eq!(state.energy_remainder_watt_ticks, 40);

        discharge_accumulator(&mut state, 122_500);
        assert_eq!(state.stored_energy_joules, 0);
        assert_eq!(state.energy_remainder_watt_ticks, 0);
    }

    #[test]
    fn allocation_is_proportional_with_ascending_leftover() {
        // Three equal-capability accumulators splitting 2 watts: floors are all
        // zero, so the two leftover watts go to the lowest entity ids.
        let entries = [entry(1, 1, 0), entry(2, 1, 0), entry(3, 1, 0)];
        let mut out = Vec::new();
        allocate_proportionally(2, &entries, &mut out, |entry| entry.charge_capability_watts);
        assert_eq!(out, vec![1, 1, 0]);
    }

    #[test]
    fn allocation_respects_capability_and_sums_to_total() {
        let entries = [entry(1, 100, 0), entry(2, 1, 0)];
        let mut out = Vec::new();
        allocate_proportionally(50, &entries, &mut out, |entry| {
            entry.charge_capability_watts
        });
        assert_eq!(out.iter().sum::<u64>(), 50);
        assert!(out[0] <= 100 && out[1] <= 1);
    }
}
