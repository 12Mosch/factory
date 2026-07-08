mod accounting;
mod demand;
mod generation;
mod topology;
mod types;

use super::*;
use accounting::*;
#[allow(unused_imports)]
use demand::*;
use generation::*;
#[allow(unused_imports)]
use topology::*;
#[allow(unused_imports)]
use types::*;

impl Simulation {
    pub(super) fn prototype_affects_power_topology(
        &self,
        prototype: &factory_data::EntityPrototype,
    ) -> bool {
        prototype.electric_pole.is_some()
            || prototype.electric_energy_source.is_some()
            || prototype.steam_engine.is_some()
            || prototype.boiler.is_some()
            || prototype.offshore_pump.is_some()
            || !prototype.fluid_boxes.is_empty()
    }

    pub(super) fn invalidate_power_state(&mut self) {
        PowerContext::new(&mut self.power).invalidate_power_state();
    }

    pub(super) fn invalidate_power_dynamic_state(&mut self) {
        PowerContext::new(&mut self.power).invalidate_power_dynamic_state();
    }

    pub(super) fn refresh_power_state(&mut self) {
        if self.power.topology_dirty {
            self.power.topology = self.rebuild_power_topology();
            self.power.topology_dirty = false;
            #[cfg(test)]
            {
                self.power.topology_rebuilds += 1;
            }
        }

        let mut networks = self.power.topology.network_accumulators();
        let consumer_demands = self.consumer_power_demands();

        for (entity_id, (active_usage_watts, drain_watts)) in &consumer_demands {
            let Some(network_id) = self
                .power
                .topology
                .network_ids_by_entity
                .get(entity_id)
                .copied()
            else {
                continue;
            };
            let network = &mut networks[network_id as usize];
            network.consumer_count += 1;
            network.consumption_watts = network
                .consumption_watts
                .saturating_add(active_usage_watts.saturating_add(*drain_watts));
        }

        let engine_assignments = self.assign_steam_engines_to_fluid_networks(
            &self.power.topology.network_ids_by_entity,
            &networks,
        );
        for assignment in engine_assignments.values() {
            let network = &mut networks[assignment.network_id as usize];
            network.producer_count += 1;
            network.available_production_watts = network
                .available_production_watts
                .saturating_add(assignment.available_power_output_watts);
        }

        for network in &mut networks {
            let (production_watts, satisfaction_permyriad) = power_satisfaction(
                network.available_production_watts,
                network.consumption_watts,
            );
            network.production_watts = production_watts;
            network.satisfaction_permyriad = satisfaction_permyriad;
        }

        let engine_output_watts = actual_steam_engine_outputs(&networks, &engine_assignments);
        self.consume_steam_for_engine_output(engine_output_watts, &engine_assignments);
        self.rebuild_fluid_networks_and_equalize();
        self.power.networks = network_snapshots(&networks);
        self.power.summary = aggregate_power_summary(&self.power.networks);
        self.power.entity_statuses = self
            .consumer_power_statuses(&self.power.topology.network_ids_by_entity, consumer_demands);
        self.record_power_sample();
    }

    #[cfg(test)]
    pub(super) fn power_topology_rebuild_count(&self) -> u64 {
        self.power.topology_rebuilds
    }
}
