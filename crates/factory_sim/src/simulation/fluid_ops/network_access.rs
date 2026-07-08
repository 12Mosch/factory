use crate::simulation::*;

use super::math::fluid_filter_accepts;
use super::types::{
    FluidBoxKey, FluidNetworkBoxTopology, FluidNetworkDynamicSummary, FluidNetworkTopology,
};

impl Simulation {
    pub(in crate::simulation) fn fluid_network_id_for_box_key(
        &self,
        key: FluidBoxKey,
    ) -> Option<u32> {
        debug_assert!(
            !self.fluids.topology_dirty,
            "fluid topology must be ensured before querying network ids"
        );
        self.fluids.network_ids_by_box.get(&key).copied()
    }

    pub(in crate::simulation) fn fluid_network_total_for_fluid(
        &self,
        network_id: u32,
        fluid_id: FluidId,
    ) -> u64 {
        let Some(network) = self.fluid_network_topology_by_id(network_id) else {
            return 0;
        };
        self.fluid_network_total_for_fluid_in_topology(network, fluid_id)
    }

    pub(in crate::simulation) fn fluid_network_available_capacity_for_fluid(
        &self,
        network_id: u32,
        fluid_id: FluidId,
    ) -> u64 {
        let Some(network) = self.fluid_network_topology_by_id(network_id) else {
            return 0;
        };
        self.fluid_network_available_capacity_for_fluid_in_topology(network, fluid_id)
    }

    pub(in crate::simulation) fn add_fluid_to_network(
        &mut self,
        network_id: u32,
        fluid_id: FluidId,
        amount_milliunits: u64,
    ) -> u64 {
        self.ensure_fluid_network_topology();
        let Some(network) = self.fluid_network_topology_by_id(network_id) else {
            return 0;
        };
        if !self.fluid_network_accepts_fluid(network, fluid_id) {
            return 0;
        }

        let mut remaining = amount_milliunits;
        let mut added = 0;
        for box_topology in &self.fluids.topology_networks[network_id as usize].boxes {
            if remaining == 0 {
                break;
            }
            if !fluid_filter_accepts(box_topology.filter, fluid_id) {
                continue;
            }
            let Some(state) = self
                .entities
                .fluid_boxes
                .get_mut(&box_topology.key.entity_id)
                .and_then(|boxes| boxes.get_mut(box_topology.key.box_index))
            else {
                continue;
            };
            if state.fluid_id.is_some_and(|existing| existing != fluid_id) {
                continue;
            }

            let available = box_topology
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
        self.ensure_fluid_network_topology();
        let Some(network) = self.fluid_network_topology_by_id(network_id) else {
            return false;
        };
        if self.fluid_network_total_for_fluid_in_topology(network, fluid_id) < amount_milliunits {
            return false;
        }

        let mut remaining = amount_milliunits;
        for box_topology in &self.fluids.topology_networks[network_id as usize].boxes {
            if remaining == 0 {
                break;
            }
            let Some(state) = self
                .entities
                .fluid_boxes
                .get_mut(&box_topology.key.entity_id)
                .and_then(|boxes| boxes.get_mut(box_topology.key.box_index))
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

    fn fluid_network_topology_by_id(&self, network_id: u32) -> Option<&FluidNetworkTopology> {
        debug_assert!(
            !self.fluids.topology_dirty,
            "fluid topology must be ensured before querying networks"
        );
        self.fluids
            .topology_networks
            .get(network_id as usize)
            .filter(|network| network.network_id == network_id)
    }

    fn fluid_network_total_for_fluid_in_topology(
        &self,
        network: &FluidNetworkTopology,
        fluid_id: FluidId,
    ) -> u64 {
        let mut scan = FluidNetworkFluidScan::default();
        let mut total_milliunits = 0_u64;

        for box_topology in &network.boxes {
            scan.observe_filter(box_topology.filter);
            if scan.blocked() {
                return 0;
            }

            let Some(state) = self
                .entities
                .fluid_boxes
                .get(&box_topology.key.entity_id)
                .and_then(|boxes| boxes.get(box_topology.key.box_index))
            else {
                continue;
            };
            scan.observe_fluid_state(state);
            if scan.blocked() {
                return 0;
            }
            if state.fluid_id == Some(fluid_id) {
                total_milliunits = total_milliunits.saturating_add(state.amount_milliunits);
            }
        }

        total_milliunits
    }

    fn fluid_network_available_capacity_for_fluid_in_topology(
        &self,
        network: &FluidNetworkTopology,
        fluid_id: FluidId,
    ) -> u64 {
        let mut scan = FluidNetworkFluidScan::default();
        let mut available_milliunits = 0_u64;

        for box_topology in &network.boxes {
            scan.observe_filter(box_topology.filter);
            if scan.blocked() {
                return 0;
            }

            let Some(state) = self
                .entities
                .fluid_boxes
                .get(&box_topology.key.entity_id)
                .and_then(|boxes| boxes.get(box_topology.key.box_index))
            else {
                continue;
            };
            scan.observe_fluid_state(state);
            if scan.blocked() {
                return 0;
            }
            if !fluid_filter_accepts(box_topology.filter, fluid_id)
                || state.fluid_id.is_some_and(|existing| existing != fluid_id)
            {
                continue;
            }
            available_milliunits = available_milliunits.saturating_add(
                box_topology
                    .capacity_milliunits
                    .saturating_sub(state.amount_milliunits),
            );
        }

        if scan.accepts_fluid(fluid_id) {
            available_milliunits
        } else {
            0
        }
    }

    fn fluid_network_accepts_fluid(
        &self,
        network: &FluidNetworkTopology,
        fluid_id: FluidId,
    ) -> bool {
        let mut scan = FluidNetworkFluidScan::default();

        for box_topology in &network.boxes {
            scan.observe_filter(box_topology.filter);
            if scan.blocked() {
                return false;
            }

            let Some(state) = self
                .entities
                .fluid_boxes
                .get(&box_topology.key.entity_id)
                .and_then(|boxes| boxes.get(box_topology.key.box_index))
            else {
                continue;
            };
            scan.observe_fluid_state(state);
            if scan.blocked() {
                return false;
            }
        }

        scan.accepts_fluid(fluid_id)
    }

    pub(super) fn fluid_network_snapshot(
        &self,
        network: &FluidNetworkTopology,
    ) -> FluidNetworkSnapshot {
        let summary = self.fluid_network_dynamic_summary(network);
        FluidNetworkSnapshot {
            network_id: network.network_id,
            fluid_id: summary.fluid_id,
            total_milliunits: summary.total_milliunits,
            capacity_milliunits: network.capacity_milliunits,
            box_count: network.boxes.len(),
            blocked: summary.blocked,
            boxes: network
                .boxes
                .iter()
                .filter_map(|box_topology| self.fluid_network_box_snapshot(box_topology))
                .collect(),
        }
    }

    pub(super) fn fluid_network_box_snapshot(
        &self,
        box_topology: &FluidNetworkBoxTopology,
    ) -> Option<FluidNetworkBoxSnapshot> {
        let key = box_topology.key;
        let state = self
            .entities
            .fluid_boxes
            .get(&key.entity_id)?
            .get(key.box_index)?;

        Some(FluidNetworkBoxSnapshot {
            entity_id: key.entity_id,
            box_index: key.box_index,
            capacity_milliunits: box_topology.capacity_milliunits,
            amount_milliunits: state.amount_milliunits,
            fluid_id: state.fluid_id,
            filter: box_topology.filter,
        })
    }

    pub(in crate::simulation) fn fluid_network_dynamic_summary(
        &self,
        network: &FluidNetworkTopology,
    ) -> FluidNetworkDynamicSummary {
        let mut scan = FluidNetworkFluidScan::default();
        let mut total_milliunits = 0_u64;

        for box_topology in &network.boxes {
            scan.observe_filter(box_topology.filter);

            let Some(state) = self
                .entities
                .fluid_boxes
                .get(&box_topology.key.entity_id)
                .and_then(|boxes| boxes.get(box_topology.key.box_index))
            else {
                continue;
            };
            total_milliunits = total_milliunits.saturating_add(state.amount_milliunits);
            scan.observe_fluid_state(state);
        }

        FluidNetworkDynamicSummary {
            total_milliunits,
            fluid_id: scan.fluid_id(),
            blocked: scan.blocked(),
        }
    }
}

#[derive(Default)]
struct FluidNetworkFluidScan {
    filter: Option<FluidId>,
    multiple_filters: bool,
    nonempty_fluid: Option<FluidId>,
    multiple_nonempty_fluids: bool,
}

impl FluidNetworkFluidScan {
    fn observe_filter(&mut self, filter: Option<FluidId>) {
        let Some(filter) = filter else {
            return;
        };
        match self.filter {
            Some(existing) if existing != filter => self.multiple_filters = true,
            None => self.filter = Some(filter),
            _ => {}
        }
    }

    fn observe_fluid_state(&mut self, state: &FluidBoxState) {
        if state.amount_milliunits == 0 {
            return;
        }
        let Some(fluid_id) = state.fluid_id else {
            return;
        };
        match self.nonempty_fluid {
            Some(existing) if existing != fluid_id => self.multiple_nonempty_fluids = true,
            None => self.nonempty_fluid = Some(fluid_id),
            _ => {}
        }
    }

    fn blocked(&self) -> bool {
        self.multiple_filters
            || self.multiple_nonempty_fluids
            || self
                .filter
                .zip(self.nonempty_fluid)
                .is_some_and(|(filter, fluid)| filter != fluid)
    }

    fn fluid_id(&self) -> Option<FluidId> {
        if self.multiple_nonempty_fluids {
            None
        } else {
            self.nonempty_fluid
                .or_else(|| (!self.multiple_filters).then_some(self.filter).flatten())
        }
    }

    fn accepts_fluid(&self, fluid_id: FluidId) -> bool {
        !self.blocked()
            && self
                .fluid_id()
                .is_none_or(|network_fluid_id| network_fluid_id == fluid_id)
    }
}
