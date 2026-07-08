use crate::simulation::*;

use super::math::per_tick_milliunits;
use super::types::FluidBoxKey;

impl Simulation {
    pub(in crate::simulation) fn advance_fluids_before_power(&mut self) {
        self.equalize_fluid_networks();
        self.advance_offshore_pumps();
        self.equalize_fluid_networks();
        self.advance_boilers();
        self.equalize_fluid_networks();
    }

    fn advance_offshore_pumps(&mut self) {
        let water = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes)
            .fluids
            .water;
        let pump_ids = self
            .entities
            .offshore_pumps
            .keys()
            .copied()
            .collect::<Vec<_>>();

        for entity_id in pump_ids {
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let Some(pump) = self
                .world
                .prototypes
                .entity(placed.prototype_id)
                .and_then(|prototype| prototype.offshore_pump.as_ref())
            else {
                continue;
            };
            let Some(network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
                entity_id,
                box_index: 0,
            }) else {
                continue;
            };

            let amount = per_tick_milliunits(pump.pumping_speed_per_second_milliunits);
            let added = self.add_fluid_to_network(network_id, water, amount);
            self.record_fluid_produced(water, added);
        }
    }

    fn advance_boilers(&mut self) {
        let ids = factory_data::BasePrototypeIds::from_catalog(&self.world.prototypes);
        let water = ids.fluids.water;
        let steam = ids.fluids.steam;
        let boiler_ids = self.entities.boilers.keys().copied().collect::<Vec<_>>();

        for entity_id in boiler_ids {
            let Some(placed) = self.entities.placed_entity(entity_id) else {
                continue;
            };
            let Some(entity_prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(boiler) = entity_prototype.boiler.as_ref() else {
                continue;
            };
            let water_amount = per_tick_milliunits(boiler.water_consumption_per_second_milliunits);
            let steam_amount = per_tick_milliunits(boiler.steam_output_per_second_milliunits);
            let Some(water_network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
                entity_id,
                box_index: 0,
            }) else {
                continue;
            };
            let Some(steam_network_id) = self.fluid_network_id_for_box_key(FluidBoxKey {
                entity_id,
                box_index: 1,
            }) else {
                continue;
            };
            if self.fluid_network_total_for_fluid(water_network_id, water) < water_amount
                || self.fluid_network_available_capacity_for_fluid(steam_network_id, steam)
                    < steam_amount
            {
                continue;
            }

            let joules_per_tick = entity_prototype
                .burner
                .as_ref()
                .map(|burner| burner.energy_usage_watts as f64 / FIXED_SIM_TICKS_PER_SECOND_F64)
                .unwrap_or(0.0);
            if joules_per_tick <= f64::EPSILON {
                continue;
            }
            let (ready, consumed_fuel) = {
                let Ok(state) = self.entities.boiler_state_mut(entity_id) else {
                    continue;
                };
                let mut consumed_fuel = Vec::new();
                while state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    let Some(item_id) = try_consume_fuel(&self.world.prototypes, &mut state.energy)
                    else {
                        break;
                    };
                    consumed_fuel.push(item_id);
                }
                if state.energy.energy_remaining_joules + f64::EPSILON < joules_per_tick {
                    if state.energy.energy_remaining_joules > 0.0 {
                        state.energy.energy_remaining_joules = 0.0;
                    }
                    (false, consumed_fuel)
                } else {
                    (true, consumed_fuel)
                }
            };
            for item_id in consumed_fuel {
                self.record_item_consumed(item_id, 1);
            }
            if !ready {
                continue;
            }
            let Ok(state) = self.entities.boiler_state_mut(entity_id) else {
                continue;
            };
            state.energy.energy_remaining_joules -= joules_per_tick;

            if !self.consume_fluid_from_network(water_network_id, water, water_amount) {
                continue;
            }
            self.record_fluid_consumed(water, water_amount);
            let added = self.add_fluid_to_network(steam_network_id, steam, steam_amount);
            self.record_fluid_produced(steam, added);
            debug_assert_eq!(added, steam_amount);
        }
    }
}
