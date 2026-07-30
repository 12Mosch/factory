use crate::heat::energy_for_temperature;
use crate::simulation::*;

/// Permyriad denominator for the reactor neighbour bonus.
const NEIGHBOUR_BONUS_FULL_PERMYRIAD: u64 = 10_000;

impl Simulation {
    /// Advances the heat network for one tick: reactors burn, the networks
    /// settle, then heat exchangers boil water into steam.
    ///
    /// The order matters. Producing before settling means a reactor's output is
    /// spread across the network the same tick it is made, and consuming after
    /// settling means every exchanger draws from a buffer holding its fair share
    /// rather than from whichever buffer happens to sit next to the reactor.
    pub(in crate::simulation) fn advance_heat_networks(&mut self) {
        self.ensure_heat_network_topology();
        self.advance_nuclear_reactors();
        self.equalize_heat_networks();
        self.advance_heat_exchangers();
        self.equalize_heat_networks();
        self.refresh_heat_network_snapshots();
    }

    fn advance_nuclear_reactors(&mut self) {
        let reactor_ids = self
            .entities
            .nuclear_reactors
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in reactor_ids {
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(reactor) = prototype.nuclear_reactor else {
                continue;
            };

            let base_joules_per_tick = reactor.heat_output_watts / SIMULATION_TICKS_PER_SECOND;
            if base_joules_per_tick == 0 {
                continue;
            }
            // A neighbour bonus is free extra heat, not extra fuel: fuel burns at
            // the base rate either way, so a reactor pair is strictly better
            // than two isolated reactors.
            let neighbour_count = self.adjacent_nuclear_reactor_count(entity_id);
            let bonus_permyriad = u64::from(reactor.neighbour_bonus_permyriad)
                .saturating_mul(u64::from(neighbour_count));
            let output_joules_per_tick = (u128::from(base_joules_per_tick)
                * u128::from(NEIGHBOUR_BONUS_FULL_PERMYRIAD + bonus_permyriad)
                / u128::from(NEIGHBOUR_BONUS_FULL_PERMYRIAD))
            .min(u128::from(u64::MAX)) as u64;

            // Refuse to burn into a full buffer: heat that cannot be stored would
            // silently vanish, and a saturated reactor should hold its fuel
            // instead (its temperature is already at maximum).
            let capacity = self.heat_buffer_capacity_joules(entity_id);
            let stored = self
                .entities
                .heat_buffers
                .get(&entity_id)
                .map_or(0, |state| state.energy_joules);
            if capacity.saturating_sub(stored) < output_joules_per_tick {
                continue;
            }

            if !self.reactor_has_burn_energy(entity_id, base_joules_per_tick as f64) {
                continue;
            }
            let Ok(state) = self.entities.nuclear_reactor_state_mut(entity_id) else {
                continue;
            };
            state.energy.energy_remaining_joules -= base_joules_per_tick as f64;

            let added = self.add_heat_to_buffer(entity_id, output_joules_per_tick);
            debug_assert_eq!(added, output_joules_per_tick);
            self.pollution_emitters.mark_active(entity_id);
        }
    }

    /// Tops the reactor's stored burn energy up to at least `joules_per_tick`,
    /// moving each consumed cell's residue into the output slot.
    ///
    /// A fuel cell is only started when its residue fits, so a reactor with a
    /// blocked output stops rather than destroying spent fuel — that is what
    /// makes reprocessing a closed loop.
    fn reactor_has_burn_energy(&mut self, entity_id: EntityId, joules_per_tick: f64) -> bool {
        let mut consumed_fuel = Vec::new();
        let mut produced_residue = Vec::new();

        let ready = {
            let catalog = &self.world.prototypes;
            let Ok(state) = self.entities.nuclear_reactors.get_mut(&entity_id).ok_or(()) else {
                return false;
            };
            while state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                let Some(fuel_item) = state.energy.fuel_slot.stack().map(|stack| stack.item_id())
                else {
                    break;
                };
                let Some(item) = catalog.item(fuel_item) else {
                    break;
                };
                if item.fuel_value_joules.is_none() {
                    break;
                }
                // Reserve room for the residue before burning; otherwise the
                // spent cell would have nowhere to go.
                if let Some(burnt_result) = item.burnt_result {
                    if state.output_slot.insert(catalog, burnt_result, 1).is_err() {
                        break;
                    }
                    produced_residue.push(burnt_result);
                }
                let Some(consumed) = try_consume_fuel(catalog, &mut state.energy) else {
                    break;
                };
                consumed_fuel.push(consumed);
            }
            state.energy.energy_remaining_joules + f64::EPSILON >= joules_per_tick
        };

        for item_id in consumed_fuel {
            self.record_item_consumed(item_id, 1);
        }
        for item_id in produced_residue {
            self.record_item_produced(item_id, 1);
        }
        ready
    }

    /// Reactors sharing a footprint edge boost each other's output.
    fn adjacent_nuclear_reactor_count(&self, entity_id: EntityId) -> u32 {
        let Some(placed) = self.entities.placed_entity(entity_id) else {
            return 0;
        };
        let footprint = placed.footprint;
        let mut neighbours = BTreeSet::new();

        let mut consider = |x: WorldTileCoord, y: WorldTileCoord| {
            if let Some(neighbour_id) = self.entities.occupancy.entity_at(x, y)
                && neighbour_id != entity_id
                && self.entities.nuclear_reactors.contains_key(&neighbour_id)
            {
                neighbours.insert(neighbour_id);
            }
        };

        for x in footprint.x..footprint.x + i64::from(footprint.width) {
            consider(x, footprint.y - 1);
            consider(x, footprint.y + i64::from(footprint.height));
        }
        for y in footprint.y..footprint.y + i64::from(footprint.height) {
            consider(footprint.x - 1, y);
            consider(footprint.x + i64::from(footprint.width), y);
        }

        neighbours.len() as u32
    }

    fn advance_heat_exchangers(&mut self) {
        let ids = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        let water = ids.fluids.water;
        let steam = ids.fluids.steam;
        let exchanger_ids = self
            .entities
            .heat_exchangers
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in exchanger_ids {
            let Some(transfer) = self.heat_exchanger_transfer(entity_id, water, steam) else {
                continue;
            };
            if !self.consume_heat_from_buffer(entity_id, transfer.energy_joules) {
                continue;
            }
            if !self.consume_fluid_from_network(
                transfer.water_network_id,
                water,
                transfer.water_milliunits,
            ) {
                // Put the drawn heat back: no water means no conversion happened.
                self.add_heat_to_buffer(entity_id, transfer.energy_joules);
                continue;
            }
            self.record_fluid_consumed(water, transfer.water_milliunits);
            let added = self.add_fluid_to_network(
                transfer.steam_network_id,
                steam,
                transfer.steam_milliunits,
            );
            debug_assert_eq!(added, transfer.steam_milliunits);
            self.record_fluid_produced(steam, added);
        }
    }

    /// The conversion a heat exchanger can perform this tick, or `None` when it
    /// is too cold, short of water, or has nowhere to put steam.
    fn heat_exchanger_transfer(
        &self,
        entity_id: EntityId,
        water: FluidId,
        steam: FluidId,
    ) -> Option<HeatExchangerTransfer> {
        let placed = self.entities.placed_entity(entity_id)?;
        let prototype = self.world.prototypes.entity(placed.prototype_id)?;
        let boiler = prototype.boiler.as_ref()?;
        let heat_source = prototype.heat_energy_source?;
        let heat_buffer = prototype.heat_buffer.as_ref()?;

        let energy_joules = heat_source.energy_usage_watts / SIMULATION_TICKS_PER_SECOND;
        if energy_joules == 0 {
            return None;
        }
        let state = self.entities.heat_buffers.get(&entity_id)?;
        // Below the minimum working temperature the exchanger produces nothing at
        // all, so a cold network has to warm up before it makes steam.
        let minimum_energy = energy_for_temperature(
            heat_source.min_working_temperature_degrees,
            heat_buffer.specific_heat_joules_per_degree,
        );
        if state.energy_joules < minimum_energy.max(energy_joules) {
            return None;
        }

        let water_milliunits = per_tick_milliunits(boiler.water_consumption_per_second_milliunits);
        let steam_milliunits = per_tick_milliunits(boiler.steam_output_per_second_milliunits);
        let water_network_id =
            self.fluid_network_id_for_box_key(FluidBoxKey::entity(entity_id, 0))?;
        let steam_network_id =
            self.fluid_network_id_for_box_key(FluidBoxKey::entity(entity_id, 1))?;
        if self.fluid_network_total_for_fluid(water_network_id, water) < water_milliunits
            || self.fluid_network_available_capacity_for_fluid(steam_network_id, steam)
                < steam_milliunits
        {
            return None;
        }

        Some(HeatExchangerTransfer {
            water_network_id,
            steam_network_id,
            water_milliunits,
            steam_milliunits,
            energy_joules,
        })
    }
}

struct HeatExchangerTransfer {
    water_network_id: u32,
    steam_network_id: u32,
    water_milliunits: u64,
    steam_milliunits: u64,
    energy_joules: u64,
}
