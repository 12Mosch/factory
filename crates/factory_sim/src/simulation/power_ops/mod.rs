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
    pub fn power_map_snapshot_in_tile_rect(
        &self,
        min_x: WorldTileCoord,
        max_x: WorldTileCoord,
        min_y: WorldTileCoord,
        max_y: WorldTileCoord,
    ) -> PowerMapSnapshot {
        if min_x > max_x || min_y > max_y {
            return PowerMapSnapshot::default();
        }
        let max_reach_x2 = self
            .world
            .prototypes
            .entities
            .iter()
            .filter_map(|prototype| prototype.electric_pole.as_ref())
            .map(|pole| pole.wire_reach_tiles_x2)
            .max()
            .unwrap_or(0);
        let reach = (i64::from(max_reach_x2) + 1) / 2;
        let candidate_ids = self.entities.occupancy.entity_ids_in_tile_rect(
            min_x.saturating_sub(reach),
            max_x.saturating_add(reach),
            min_y.saturating_sub(reach),
            max_y.saturating_add(reach),
        );
        let poles = candidate_ids
            .iter()
            .filter(|id| self.entities.electric_poles.contains_key(id))
            .filter_map(|id| {
                let placed = self.entities.placed_entity(*id)?;
                let prototype = self
                    .world
                    .prototypes
                    .entity(placed.prototype_id)?
                    .electric_pole
                    .as_ref()?;
                let (center_x2, center_y2) = footprint_center_x2(&placed.footprint);
                Some(PoleNode {
                    entity_id: *id,
                    placed,
                    prototype,
                    center_x2,
                    center_y2,
                })
            })
            .collect::<Vec<_>>();
        let rebuilt_topology;
        let topology = if self.power.topology_dirty {
            rebuilt_topology = self.rebuild_power_topology();
            &rebuilt_topology
        } else {
            &self.power.topology
        };
        let satisfaction = |network_id: u32| {
            self.power
                .networks
                .get(network_id as usize)
                .map_or(0, |network| network.satisfaction_permyriad)
        };
        let mut snapshot = PowerMapSnapshot::default();
        for pole in &poles {
            let Some(network_id) = topology.network_ids_by_entity.get(&pole.entity_id).copied()
            else {
                continue;
            };
            snapshot.poles.push(PowerMapPole {
                entity_id: pole.entity_id,
                center_x2: pole.center_x2,
                center_y2: pole.center_y2,
                network_id,
                satisfaction_permyriad: satisfaction(network_id),
            });
        }
        let bucket_span = i64::from(max_reach_x2.max(1));
        let mut buckets = BTreeMap::<(WorldTileCoord, WorldTileCoord), Vec<usize>>::new();
        for (index, pole) in poles.iter().enumerate() {
            buckets
                .entry((
                    pole.center_x2.div_euclid(bucket_span),
                    pole.center_y2.div_euclid(bucket_span),
                ))
                .or_default()
                .push(index);
        }
        for (index, pole) in poles.iter().enumerate() {
            let bx = pole.center_x2.div_euclid(bucket_span);
            let by = pole.center_y2.div_euclid(bucket_span);
            for y in by - 1..=by + 1 {
                for x in bx - 1..=bx + 1 {
                    let Some(candidates) = buckets.get(&(x, y)) else {
                        continue;
                    };
                    for &other_index in candidates {
                        if other_index <= index
                            || !poles_are_within_mutual_reach(pole, &poles[other_index])
                        {
                            continue;
                        }
                        let other = &poles[other_index];
                        let Some(network_id) =
                            topology.network_ids_by_entity.get(&pole.entity_id).copied()
                        else {
                            continue;
                        };
                        snapshot.connections.push(PowerMapConnection {
                            first_pole_id: pole.entity_id.min(other.entity_id),
                            second_pole_id: pole.entity_id.max(other.entity_id),
                            network_id,
                            satisfaction_permyriad: satisfaction(network_id),
                        });
                    }
                }
            }
        }
        snapshot
            .connections
            .sort_by_key(|link| (link.first_pole_id, link.second_pole_id));
        for entity_id in self
            .entities
            .occupancy
            .entity_ids_in_tile_rect(min_x, max_x, min_y, max_y)
        {
            if !self.entities.electric_consumers.contains_key(&entity_id) {
                continue;
            }
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let network_id = topology.network_ids_by_entity.get(&entity_id).copied();
            snapshot.consumers.push(PowerMapConsumer {
                entity_id,
                footprint: placed.footprint,
                network_id,
                satisfaction_permyriad: network_id.map_or(0, satisfaction),
            });
        }
        snapshot
    }

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
        self.power_demand_cache.invalidate();
    }

    pub(super) fn invalidate_power_dynamic_state(&mut self) {
        PowerContext::new(&mut self.power).invalidate_power_dynamic_state();
        self.power_demand_cache.invalidate();
    }

    pub(super) fn invalidate_consumer_power_demand(&mut self, entity_id: EntityId) {
        self.power_demand_cache.mark_dirty(entity_id);
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

        refresh_consumer_demand_cache(
            &self.world,
            &self.entities,
            &self.research,
            &self.power.topology,
            &mut self.power.entity_statuses,
            &mut self.power_demand_cache,
            &mut networks,
        );

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
        if self.consume_steam_for_engine_output(engine_output_watts, &engine_assignments) {
            // Machine systems consume their own box state later in this tick,
            // so redistribute changed steam now. Durable fluid snapshots are
            // refreshed once after all machines have run.
            self.equalize_fluid_networks();
        }
        self.power.networks = network_snapshots(&networks);
        self.power.summary = aggregate_power_summary(&self.power.networks);
        if self.any_steam_engine_can_generate(&self.power.topology.network_ids_by_entity) {
            self.onboarding_progress.record_electricity_generated();
        }
        for (network_index, consumers) in self
            .power_demand_cache
            .consumers_by_network
            .iter()
            .enumerate()
        {
            let satisfaction_permyriad = self.power.networks[network_index].satisfaction_permyriad;
            if self.power_demand_cache.network_satisfaction_permyriad[network_index]
                == satisfaction_permyriad
            {
                continue;
            }
            self.power_demand_cache.network_satisfaction_permyriad[network_index] =
                satisfaction_permyriad;
            for entity_id in consumers {
                if let Some(status) = self.power.entity_statuses.get_mut(entity_id) {
                    status.satisfaction_permyriad = satisfaction_permyriad;
                }
            }
        }
        self.record_power_sample();
    }

    #[cfg(test)]
    pub(super) fn power_topology_rebuild_count(&self) -> u64 {
        self.power.topology_rebuilds
    }
}
