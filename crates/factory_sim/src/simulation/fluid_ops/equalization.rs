use crate::simulation::*;

use super::math::proportional_amount;
use super::network_access::{fluid_network_dynamic_summary, update_fluid_network_snapshot};
use super::network_builder::build_fluid_network_topology_from_nodes;
use super::types::{
    FluidBoxAssignment, FluidBoxNode, FluidBoxes, FluidBoxesMut, FluidNetworkTopology,
};
use crate::simulation::edge_geometry::{
    EdgeEndpoint, rotated_edge_connection_geometry, rotated_edge_endpoint,
};

impl Simulation {
    pub(in crate::simulation) fn ensure_fluid_network_topology(&mut self) {
        if !self.fluids.topology_dirty {
            return;
        }

        let topology_networks = self.build_fluid_network_topology();
        self.fluids.replace_topology(topology_networks);
        #[cfg(test)]
        {
            self.fluids.topology_rebuilds += 1;
        }
    }

    pub(in crate::simulation) fn equalize_fluid_networks(&mut self) {
        self.ensure_fluid_network_topology();
        for network_index in 0..self.fluids.topology_networks.len() {
            if !self.fluids.networks_needing_equalization[network_index] {
                continue;
            }
            self.fluids.networks_needing_equalization[network_index] = false;
            self.fluids.networks_needing_snapshot[network_index] = true;
            let Simulation {
                entities,
                rolling_stock,
                fluids,
                ..
            } = self;
            equalize_fluid_network(
                FluidBoxesMut::new(entities, rolling_stock),
                &fluids.topology_networks[network_index],
                &mut fluids.equalization_assignments,
            );
        }
    }

    pub(in crate::simulation) fn refresh_fluid_network_snapshots(&mut self) {
        self.ensure_fluid_network_topology();
        self.fluids.networks.resize(
            self.fluids.topology_networks.len(),
            FluidNetworkSnapshot::default(),
        );
        for network_index in 0..self.fluids.topology_networks.len() {
            if !self.fluids.networks_needing_snapshot[network_index] {
                continue;
            }
            self.fluids.networks_needing_snapshot[network_index] = false;
            let Simulation {
                entities,
                rolling_stock,
                fluids,
                ..
            } = self;
            update_fluid_network_snapshot(
                FluidBoxes::new(entities, rolling_stock),
                &fluids.topology_networks[network_index],
                &mut fluids.networks[network_index],
            );
        }
    }

    pub(in crate::simulation) fn refresh_fluid_networks_after_dynamic_changes(&mut self) {
        self.equalize_fluid_networks();
        self.refresh_fluid_network_snapshots();
    }

    fn build_fluid_network_topology(&self) -> Vec<FluidNetworkTopology> {
        let nodes = self.fluid_box_nodes();
        build_fluid_network_topology_from_nodes(&nodes)
    }

    fn fluid_box_nodes(&self) -> Vec<FluidBoxNode> {
        let mut nodes = Vec::new();
        let underground_pairs = self.underground_pipe_pairs();
        for placed in self.entities.placed_entities.values() {
            if !self.entities.fluid_boxes.contains_key(&placed.id) {
                continue;
            }
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };

            for (box_index, fluid_box) in prototype.fluid_boxes.iter().enumerate() {
                let endpoints = fluid_box
                    .connections
                    .iter()
                    .filter_map(|connection| rotated_edge_endpoint(placed, prototype, connection))
                    .collect();
                nodes.push(FluidBoxNode {
                    key: FluidBoxKey::entity(placed.id, box_index),
                    capacity_milliunits: fluid_box.capacity_milliunits,
                    filter: fluid_box.filter,
                    endpoints,
                    underground_pairs: underground_pairs
                        .get(&placed.id)
                        .cloned()
                        .unwrap_or_default(),
                });
            }
        }
        nodes.extend(self.stopped_stock_fluid_nodes());
        nodes
    }

    /// The fluid boxes of stopped rolling stock a pump reaches, as network
    /// nodes.
    ///
    /// A wagon declares no fluid connections of its own, and that is the whole
    /// design: its tank is filled and drained at a station, not by touching a
    /// pipe. So the *pump* is what reaches out — a pump connection whose facing
    /// tile a stopped wagon lies over joins that wagon's tank onto the same
    /// edge endpoint, which is exactly the adjacency two pipes would have. That
    /// is what puts the wagon on the pump's own side of the transfer rather
    /// than straight onto the main line: a wagon at a pump's output is filled
    /// at the pump's rate, and one at its input drains at that rate, instead of
    /// equalizing with the whole pipe network the instant it stops.
    ///
    /// Only pumps, deliberately. A pipe run that happened to pass a siding
    /// would otherwise start filling any train parked beside it.
    ///
    /// The search is the pump's, not the wagon's, so it has to walk the placed
    /// entities to find the pumps — but only when there is something for a pump
    /// to find. A railway with no fluid wagon standing anywhere answers in the
    /// time it takes to look at the index, which is what a topology rebuild in
    /// a factory of thousands of entities and no parked tanker should cost.
    fn stopped_stock_fluid_nodes(&self) -> Vec<FluidBoxNode> {
        if !self.any_stopped_stock_carries_fluid() {
            return Vec::new();
        }

        let mut endpoints_by_stock = BTreeMap::<RollingStockId, Vec<EdgeEndpoint>>::new();
        for placed in self.entities.placed_entities.values() {
            let Some(prototype) = self
                .world
                .prototypes
                .entity(placed.prototype_id)
                .filter(|prototype| prototype.pump.is_some())
            else {
                continue;
            };
            for connection in prototype
                .fluid_boxes
                .iter()
                .flat_map(|fluid_box| &fluid_box.connections)
            {
                let Some(geometry) =
                    rotated_edge_connection_geometry(placed, prototype, connection)
                else {
                    continue;
                };
                let Some(stock) = self
                    .stopped_stock()
                    .at(geometry.facing_tile.0, geometry.facing_tile.1)
                    .filter(|stock| !stock.fluid_boxes.is_empty())
                else {
                    continue;
                };
                let endpoints = endpoints_by_stock.entry(stock.id).or_default();
                if !endpoints.contains(&geometry.endpoint) {
                    endpoints.push(geometry.endpoint);
                }
            }
        }

        endpoints_by_stock
            .into_iter()
            .flat_map(|(stock_id, endpoints)| {
                let prototype = self
                    .rolling_stock
                    .get(stock_id)
                    .and_then(|stock| self.world.prototypes.entity(stock.prototype_id));
                prototype
                    .into_iter()
                    .flat_map(move |prototype| prototype.fluid_boxes.iter().enumerate())
                    .map(move |(box_index, fluid_box)| FluidBoxNode {
                        key: FluidBoxKey::rolling_stock(stock_id, box_index),
                        capacity_milliunits: fluid_box.capacity_milliunits,
                        filter: fluid_box.filter,
                        // Every declared tank on the piece shares the reaching
                        // connections. Base rolling stock declares exactly one,
                        // so this is the whole of it; a catalog that declared
                        // more would have them all reachable rather than
                        // silently leaving all but the first unfillable.
                        endpoints: endpoints.clone(),
                        underground_pairs: Vec::new(),
                    })
            })
            .collect()
    }

    /// Which rolling-stock fluid boxes the networks are expected to account
    /// for. Validation asks, so that it holds the snapshots to the same rule
    /// the topology is built by rather than to a second copy of it.
    pub(in crate::simulation) fn networked_rolling_stock_fluid_boxes(
        &self,
    ) -> impl Iterator<Item = (RollingStockId, usize)> + '_ {
        self.stopped_stock_fluid_nodes()
            .into_iter()
            .filter_map(|node| {
                node.key
                    .owner
                    .rolling_stock_id()
                    .map(|stock_id| (stock_id, node.key.box_index))
            })
    }

    fn underground_pipe_pairs(&self) -> BTreeMap<EntityId, Vec<(EntityId, EntityId)>> {
        let mut pairs_by_entity = BTreeMap::<EntityId, Vec<(EntityId, EntityId)>>::new();
        for placed in self.entities.placed_entities.values() {
            let Some(prototype) = self.world.prototypes.entity(placed.prototype_id) else {
                continue;
            };
            let Some(underground) = prototype
                .underground_pipe
                .as_ref()
                .filter(|underground| underground.part == UndergroundBeltPart::Entrance)
            else {
                continue;
            };
            let Some(candidate_id) = paired_underground_entity(
                &self.entities,
                placed,
                UndergroundEndpoint {
                    part: underground.part,
                    max_distance: underground.max_distance,
                },
                |candidate_id| {
                    let candidate = self.entities.placed_entity(candidate_id)?;
                    let candidate_pipe = self
                        .world
                        .prototypes
                        .entity(candidate.prototype_id)?
                        .underground_pipe
                        .as_ref()?;
                    Some(UndergroundEndpoint {
                        part: candidate_pipe.part,
                        max_distance: candidate_pipe.max_distance,
                    })
                },
            ) else {
                continue;
            };
            let pair = (placed.id, candidate_id);
            pairs_by_entity.entry(placed.id).or_default().push(pair);
            pairs_by_entity.entry(candidate_id).or_default().push(pair);
        }
        pairs_by_entity
    }
}

fn equalize_fluid_network(
    mut boxes: FluidBoxesMut<'_>,
    network: &FluidNetworkTopology,
    assignments: &mut Vec<FluidBoxAssignment>,
) {
    if network.boxes.is_empty() || network.capacity_milliunits == 0 {
        return;
    }

    let summary = fluid_network_dynamic_summary(boxes.as_ref(), network);
    if summary.blocked {
        return;
    }

    if summary.total_milliunits == 0 {
        for box_topology in &network.boxes {
            if let Some(state) = boxes.get_mut(box_topology.key) {
                state.amount_milliunits = 0;
                state.fluid_id = None;
            }
        }
        return;
    }
    let Some(fluid_id) = summary.fluid_id else {
        return;
    };

    assignments.clear();
    let mut assigned_total = 0_u64;
    for box_topology in &network.boxes {
        let key = box_topology.key;
        let capacity = box_topology.capacity_milliunits;
        let assigned = proportional_amount(
            summary.total_milliunits,
            capacity,
            network.capacity_milliunits,
        );
        assignments.push(FluidBoxAssignment {
            key,
            capacity_milliunits: capacity,
            amount_milliunits: assigned,
        });
        assigned_total = assigned_total.saturating_add(assigned);
    }

    let mut remainder = summary.total_milliunits.saturating_sub(assigned_total);
    for assignment in assignments.iter_mut() {
        if remainder == 0 {
            break;
        }
        if assignment.amount_milliunits < assignment.capacity_milliunits {
            assignment.amount_milliunits += 1;
            remainder -= 1;
        }
    }

    debug_assert_eq!(remainder, 0);
    for assignment in assignments.iter() {
        let assigned = assignment.amount_milliunits;
        if let Some(state) = boxes.get_mut(assignment.key) {
            state.amount_milliunits = assigned;
            state.fluid_id = (assigned > 0).then_some(fluid_id);
        }
    }
}
