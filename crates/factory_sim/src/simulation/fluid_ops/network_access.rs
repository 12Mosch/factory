use crate::simulation::*;

use super::math::fluid_filter_accepts;
use super::types::{BuiltFluidNetwork, FluidBoxKey};

impl Simulation {
    pub(in crate::simulation) fn fluid_network_id_for_box_key(
        &self,
        key: FluidBoxKey,
    ) -> Option<u32> {
        self.fluids
            .networks
            .iter()
            .find(|network| {
                network.boxes.iter().any(|box_snapshot| {
                    box_snapshot.entity_id == key.entity_id
                        && box_snapshot.box_index == key.box_index
                })
            })
            .map(|network| network.network_id)
    }

    pub(in crate::simulation) fn fluid_network_total_for_fluid(
        &self,
        network_id: u32,
        fluid_id: FluidId,
    ) -> u64 {
        let Some(network) = self.fluid_network_by_id(network_id) else {
            return 0;
        };
        if network.blocked {
            return 0;
        }

        network
            .boxes
            .iter()
            .filter_map(|box_snapshot| {
                let state = self
                    .entities
                    .fluid_boxes
                    .get(&box_snapshot.entity_id)?
                    .get(box_snapshot.box_index)?;
                (state.fluid_id == Some(fluid_id)).then_some(state.amount_milliunits)
            })
            .sum()
    }

    pub(in crate::simulation) fn fluid_network_available_capacity_for_fluid(
        &self,
        network_id: u32,
        fluid_id: FluidId,
    ) -> u64 {
        let Some(network) = self.fluid_network_by_id(network_id) else {
            return 0;
        };
        if network.blocked {
            return 0;
        }

        network
            .boxes
            .iter()
            .filter_map(|box_snapshot| {
                if !fluid_filter_accepts(box_snapshot.filter, fluid_id) {
                    return None;
                }
                let state = self
                    .entities
                    .fluid_boxes
                    .get(&box_snapshot.entity_id)?
                    .get(box_snapshot.box_index)?;
                if state.fluid_id.is_some_and(|existing| existing != fluid_id) {
                    return None;
                }
                Some(
                    box_snapshot
                        .capacity_milliunits
                        .saturating_sub(state.amount_milliunits),
                )
            })
            .sum()
    }

    pub(in crate::simulation) fn add_fluid_to_network(
        &mut self,
        network_id: u32,
        fluid_id: FluidId,
        amount_milliunits: u64,
    ) -> u64 {
        let Some(network) = self.fluid_network_by_id(network_id).cloned() else {
            return 0;
        };
        if network.blocked {
            return 0;
        }

        let mut remaining = amount_milliunits;
        let mut added = 0;
        for box_snapshot in network.boxes {
            if remaining == 0 {
                break;
            }
            if !fluid_filter_accepts(box_snapshot.filter, fluid_id) {
                continue;
            }
            let Some(state) = self
                .entities
                .fluid_boxes
                .get_mut(&box_snapshot.entity_id)
                .and_then(|boxes| boxes.get_mut(box_snapshot.box_index))
            else {
                continue;
            };
            if state.fluid_id.is_some_and(|existing| existing != fluid_id) {
                continue;
            }

            let available = box_snapshot
                .capacity_milliunits
                .saturating_sub(state.amount_milliunits);
            let inserted = available.min(remaining);
            if inserted == 0 {
                continue;
            }
            state.fluid_id = Some(fluid_id);
            state.amount_milliunits += inserted;
            remaining -= inserted;
            added += inserted;
        }

        added
    }

    pub(in crate::simulation) fn consume_fluid_from_network(
        &mut self,
        network_id: u32,
        fluid_id: FluidId,
        amount_milliunits: u64,
    ) -> bool {
        if self.fluid_network_total_for_fluid(network_id, fluid_id) < amount_milliunits {
            return false;
        }

        let Some(network) = self.fluid_network_by_id(network_id).cloned() else {
            return false;
        };
        let mut remaining = amount_milliunits;
        for box_snapshot in network.boxes {
            if remaining == 0 {
                break;
            }
            let Some(state) = self
                .entities
                .fluid_boxes
                .get_mut(&box_snapshot.entity_id)
                .and_then(|boxes| boxes.get_mut(box_snapshot.box_index))
            else {
                continue;
            };
            if state.fluid_id != Some(fluid_id) {
                continue;
            }

            let removed = state.amount_milliunits.min(remaining);
            state.amount_milliunits -= removed;
            if state.amount_milliunits == 0 {
                state.fluid_id = None;
            }
            remaining -= removed;
        }

        debug_assert_eq!(remaining, 0);
        true
    }

    pub(super) fn fluid_network_by_id(&self, network_id: u32) -> Option<&FluidNetworkSnapshot> {
        self.fluids
            .networks
            .get(network_id as usize)
            .filter(|network| network.network_id == network_id)
    }

    pub(super) fn fluid_network_snapshot(
        &self,
        network: &BuiltFluidNetwork,
    ) -> FluidNetworkSnapshot {
        FluidNetworkSnapshot {
            network_id: network.network_id,
            fluid_id: network.fluid_id,
            total_milliunits: network.total_milliunits,
            capacity_milliunits: network.capacity_milliunits,
            box_count: network.boxes.len(),
            blocked: network.blocked,
            boxes: network
                .boxes
                .iter()
                .filter_map(|key| self.fluid_network_box_snapshot(*key))
                .collect(),
        }
    }

    pub(super) fn fluid_network_box_snapshot(
        &self,
        key: FluidBoxKey,
    ) -> Option<FluidNetworkBoxSnapshot> {
        let placed = self.entities.placed_entity(key.entity_id)?;
        let prototype = self.world.prototypes.entity(placed.prototype_id)?;
        let fluid_box = prototype.fluid_boxes.get(key.box_index)?;
        let state = self
            .entities
            .fluid_boxes
            .get(&key.entity_id)?
            .get(key.box_index)?;

        Some(FluidNetworkBoxSnapshot {
            entity_id: key.entity_id,
            box_index: key.box_index,
            capacity_milliunits: fluid_box.capacity_milliunits,
            amount_milliunits: state.amount_milliunits,
            fluid_id: state.fluid_id,
            filter: fluid_box.filter,
        })
    }

    pub(super) fn fluid_box_capacity(&self, key: FluidBoxKey) -> Option<u64> {
        let placed = self.entities.placed_entity(key.entity_id)?;
        self.world
            .prototypes
            .entity(placed.prototype_id)?
            .fluid_boxes
            .get(key.box_index)
            .map(|fluid_box| fluid_box.capacity_milliunits)
    }
}
