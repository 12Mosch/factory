use crate::simulation::*;

use super::math::per_tick_milliunits;
use super::network_access::{
    fluid_network_available_capacity_for_fluid, fluid_network_fluid_id,
    fluid_network_id_for_box_key, fluid_network_total_for_fluid,
};
use super::types::FluidBoxKey;

impl Simulation {
    pub(in crate::simulation) fn advance_fluids_before_power(&mut self) {
        self.equalize_fluid_networks();
        self.advance_offshore_pumps();
        self.equalize_fluid_networks();
        self.advance_boilers();
        self.equalize_fluid_networks();
    }

    pub(in crate::simulation) fn advance_fluid_pumps_after_power(&mut self) {
        self.ensure_fluid_network_topology();
        let pump_ids = self
            .entities
            .placed_entities
            .values()
            .filter_map(|placed| {
                self.world
                    .prototypes
                    .entity(placed.prototype_id)
                    .and_then(|prototype| prototype.pump.as_ref())
                    .map(|_| placed.id)
            })
            .collect::<Vec<_>>();

        for entity_id in pump_ids {
            let Some(transfer) = pump_fluid_transfer(
                &self.world.prototypes,
                &self.entities,
                &self.fluids,
                entity_id,
            ) else {
                continue;
            };
            if !electric_work_allowed_for(
                &self.power,
                &mut self.entities.electric_consumers,
                entity_id,
            ) {
                continue;
            }
            if !self.consume_fluid_from_network(
                transfer.input_network_id,
                transfer.fluid_id,
                transfer.amount_milliunits,
            ) {
                continue;
            }
            let added = self.add_fluid_to_network(
                transfer.output_network_id,
                transfer.fluid_id,
                transfer.amount_milliunits,
            );
            debug_assert_eq!(added, transfer.amount_milliunits);
        }
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
            if added > 0 {
                self.pollution_emitters.mark_active(entity_id);
            }
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
            self.pollution_emitters.mark_active(entity_id);
        }
    }
}

/// The fluid transfer a pump would perform this tick, ignoring its power supply.
pub(in crate::simulation) struct PumpFluidTransfer {
    pub(in crate::simulation) input_network_id: u32,
    pub(in crate::simulation) output_network_id: u32,
    pub(in crate::simulation) fluid_id: FluidId,
    pub(in crate::simulation) amount_milliunits: u64,
}

/// Returns the transfer a pump can perform, or `None` when it has nothing to move.
///
/// Shared by the pump simulation step and the electric demand estimate so that an idle pump
/// is never billed for active power usage. The fluid network topology must be current.
pub(in crate::simulation) fn pump_fluid_transfer(
    catalog: &PrototypeCatalog,
    entities: &EntityStore,
    fluids: &FluidSubsystem,
    entity_id: EntityId,
) -> Option<PumpFluidTransfer> {
    let placed = entities.placed_entity(entity_id)?;
    let pump = catalog.entity(placed.prototype_id)?.pump.as_ref()?;
    let input_network_id = fluid_network_id_for_box_key(
        fluids,
        FluidBoxKey {
            entity_id,
            box_index: 0,
        },
    )?;
    let output_network_id = fluid_network_id_for_box_key(
        fluids,
        FluidBoxKey {
            entity_id,
            box_index: 1,
        },
    )?;
    if input_network_id == output_network_id {
        return None;
    }

    let fluid_id = fluid_network_fluid_id(fluids, entities, input_network_id)?;
    let amount_milliunits = per_tick_milliunits(pump.pumping_speed_per_second_milliunits)
        .min(fluid_network_total_for_fluid(
            fluids,
            entities,
            input_network_id,
            fluid_id,
        ))
        .min(fluid_network_available_capacity_for_fluid(
            fluids,
            entities,
            output_network_id,
            fluid_id,
        ));
    (amount_milliunits > 0).then_some(PumpFluidTransfer {
        input_network_id,
        output_network_id,
        fluid_id,
        amount_milliunits,
    })
}
