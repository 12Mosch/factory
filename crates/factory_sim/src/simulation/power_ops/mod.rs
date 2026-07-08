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

        self.ensure_fluid_network_topology();
        let mut networks = self.power.topology.network_accumulators();

        let catalog = &self.world.prototypes;
        let entities = &self.entities;
        let research = &self.research;
        let network_ids_by_entity = &self.power.topology.network_ids_by_entity;
        self.power.entity_statuses.retain(|entity_id, _| {
            entities.electric_consumers.contains_key(entity_id)
                && electric_consumer_has_power_source(catalog, entities, *entity_id)
        });

        for &entity_id in entities.electric_consumers.keys() {
            let Some((active_usage_watts, drain_watts)) =
                consumer_power_demand_for(catalog, entities, research, entity_id)
            else {
                continue;
            };
            let network_id = network_ids_by_entity.get(&entity_id).copied();
            let status = self.power.entity_statuses.entry(entity_id).or_default();
            status.network_id = network_id;
            status.active_usage_watts = active_usage_watts;
            status.drain_watts = drain_watts;

            if let Some(network_id) = network_id {
                let network = &mut networks[network_id as usize];
                network.consumer_count += 1;
                network.consumption_watts = network
                    .consumption_watts
                    .saturating_add(active_usage_watts.saturating_add(drain_watts));
            }
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
        self.refresh_fluid_networks_after_dynamic_changes();
        self.power.networks = network_snapshots(&networks);
        self.power.summary = aggregate_power_summary(&self.power.networks);
        for status in self.power.entity_statuses.values_mut() {
            status.satisfaction_permyriad = status
                .network_id
                .and_then(|network_id| self.power.networks.get(network_id as usize))
                .map(|network| network.satisfaction_permyriad)
                .unwrap_or(0);
        }
        self.record_power_sample();
    }

    #[cfg(test)]
    pub(super) fn power_topology_rebuild_count(&self) -> u64 {
        self.power.topology_rebuilds
    }
}
