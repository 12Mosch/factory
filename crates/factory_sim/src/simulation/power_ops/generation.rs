use std::collections::BTreeMap;

use super::types::{NetworkAccumulator, SteamEngineAssignment};
use super::*;

impl Simulation {
    pub(super) fn assign_steam_engines_to_fluid_networks(
        &self,
        network_ids_by_entity: &BTreeMap<EntityId, u32>,
        networks: &[NetworkAccumulator],
    ) -> BTreeMap<EntityId, SteamEngineAssignment> {
        let steam = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .steam;
        let mut assignments = BTreeMap::new();
        let mut remaining_demand_by_network = networks
            .iter()
            .enumerate()
            .map(|(network_id, network)| (network_id as u32, network.consumption_watts))
            .collect::<BTreeMap<_, _>>();
        let mut remaining_steam_by_network = self
            .fluids
            .topology_networks
            .iter()
            .filter_map(|network| {
                let summary = self.fluid_network_dynamic_summary(network);
                (!summary.blocked && summary.fluid_id == Some(steam))
                    .then_some((network.network_id, summary.total_milliunits))
            })
            .collect::<BTreeMap<_, _>>();

        for engine_id in self.entities.steam_engines.keys().copied() {
            let Some(network_id) = network_ids_by_entity.get(&engine_id).copied() else {
                continue;
            };
            let Some(remaining_demand) = remaining_demand_by_network.get_mut(&network_id) else {
                continue;
            };
            if *remaining_demand == 0 {
                continue;
            }
            let Some(engine_prototype) = self.steam_engine_prototype(engine_id) else {
                continue;
            };
            let Some(steam_network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
                entity_id: engine_id,
                box_index: 0,
            }) else {
                continue;
            };
            let Some(remaining_steam) = remaining_steam_by_network.get_mut(&steam_network_id)
            else {
                continue;
            };
            let steam_consumption_per_tick_milliunits =
                per_tick_milliunits(engine_prototype.steam_consumption_per_second_milliunits);
            if steam_consumption_per_tick_milliunits == 0 || *remaining_steam == 0 {
                continue;
            }

            let demand_limited_output =
                (*remaining_demand).min(engine_prototype.max_power_output_watts);
            let demand_limited_steam_budget = steam_consumed_for_output(
                demand_limited_output,
                engine_prototype.max_power_output_watts,
                steam_consumption_per_tick_milliunits,
            );
            let steam_budget_milliunits = (*remaining_steam)
                .min(steam_consumption_per_tick_milliunits)
                .min(demand_limited_steam_budget);
            if steam_budget_milliunits == 0 {
                continue;
            }
            let available_power_output_watts = engine_prototype
                .max_power_output_watts
                .saturating_mul(steam_budget_milliunits)
                / steam_consumption_per_tick_milliunits;
            if available_power_output_watts == 0 {
                continue;
            }
            *remaining_steam -= steam_budget_milliunits;
            *remaining_demand = remaining_demand.saturating_sub(available_power_output_watts);
            assignments.insert(
                engine_id,
                SteamEngineAssignment {
                    network_id,
                    steam_network_id,
                    available_power_output_watts,
                    max_power_output_watts: engine_prototype.max_power_output_watts,
                    steam_budget_milliunits,
                    steam_consumption_per_tick_milliunits,
                },
            );
        }

        assignments
    }

    /// True when at least one steam engine is wired into a power network and
    /// has usable steam flowing into it, regardless of whether any consumer
    /// currently demands that power. `assign_steam_engines_to_fluid_networks`
    /// caps assignment on network demand, so it can't answer this on its own
    /// (a generator built before any consumer would otherwise never report
    /// as "generating").
    pub(super) fn any_steam_engine_can_generate(
        &self,
        network_ids_by_entity: &BTreeMap<EntityId, u32>,
    ) -> bool {
        let steam = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .steam;

        self.entities.steam_engines.keys().any(|&engine_id| {
            network_ids_by_entity.contains_key(&engine_id)
                && self
                    .steam_engine_prototype(engine_id)
                    .is_some_and(|prototype| {
                        per_tick_milliunits(prototype.steam_consumption_per_second_milliunits) > 0
                    })
                && self
                    .fluid_network_id_for_box_key(FluidBoxKey {
                        entity_id: engine_id,
                        box_index: 0,
                    })
                    .and_then(|steam_network_id| {
                        self.fluids
                            .topology_networks
                            .iter()
                            .find(|network| network.network_id == steam_network_id)
                    })
                    .is_some_and(|network| {
                        let summary = self.fluid_network_dynamic_summary(network);
                        !summary.blocked
                            && summary.fluid_id == Some(steam)
                            && summary.total_milliunits > 0
                    })
        })
    }

    pub(in crate::simulation) fn steam_engine_prototype(
        &self,
        engine_id: EntityId,
    ) -> Option<&factory_data::SteamEnginePrototype> {
        let placed = self.entities.placed_entity(engine_id)?;
        self.world
            .prototypes
            .entity(placed.prototype_id)?
            .steam_engine
            .as_ref()
    }

    pub(super) fn consume_steam_for_engine_output(
        &mut self,
        engine_output_watts: BTreeMap<EntityId, u64>,
        engine_assignments: &BTreeMap<EntityId, SteamEngineAssignment>,
    ) {
        let steam = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .steam;
        for (engine_id, output_watts) in engine_output_watts {
            if output_watts == 0 {
                continue;
            }
            let Some(assignment) = engine_assignments.get(&engine_id) else {
                continue;
            };
            let steam_to_consume = steam_consumed_for_output(
                output_watts,
                assignment.max_power_output_watts,
                assignment.steam_consumption_per_tick_milliunits,
            )
            .min(assignment.steam_budget_milliunits);
            if steam_to_consume > 0
                && self.consume_fluid_from_network(
                    assignment.steam_network_id,
                    steam,
                    steam_to_consume,
                )
            {
                self.pollution_emitters.mark_active(engine_id);
                self.record_fluid_consumed(steam, steam_to_consume);
            }
        }
    }
}

pub(super) fn actual_steam_engine_outputs(
    networks: &[NetworkAccumulator],
    engine_assignments: &BTreeMap<EntityId, SteamEngineAssignment>,
) -> BTreeMap<EntityId, u64> {
    let mut output_by_engine = BTreeMap::<EntityId, u64>::new();
    let mut engines_by_network = BTreeMap::<u32, Vec<(EntityId, SteamEngineAssignment)>>::new();

    for (engine_id, assignment) in engine_assignments {
        engines_by_network
            .entry(assignment.network_id)
            .or_default()
            .push((*engine_id, *assignment));
    }

    for (network_id, engines) in engines_by_network {
        let Some(network) = networks.get(network_id as usize) else {
            continue;
        };
        let mut remaining_production = network.production_watts;
        let mut remaining_available = network.available_production_watts;

        for (engine_id, assignment) in engines {
            if remaining_available == 0 || remaining_production == 0 {
                break;
            }
            let actual_output = assignment
                .available_power_output_watts
                .saturating_mul(remaining_production)
                / remaining_available;
            remaining_production = remaining_production.saturating_sub(actual_output);
            remaining_available =
                remaining_available.saturating_sub(assignment.available_power_output_watts);
            output_by_engine.insert(engine_id, actual_output);
        }
    }

    output_by_engine
}

pub(super) fn steam_consumed_for_output(
    output_watts: u64,
    max_power_output_watts: u64,
    steam_consumption_per_tick_milliunits: u64,
) -> u64 {
    if output_watts == 0 || max_power_output_watts == 0 {
        return 0;
    }

    let numerator = u128::from(steam_consumption_per_tick_milliunits) * u128::from(output_watts);
    let denominator = u128::from(max_power_output_watts);
    numerator.div_ceil(denominator) as u64
}
