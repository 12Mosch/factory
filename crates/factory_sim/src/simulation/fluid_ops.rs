use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct FluidBoxKey {
    pub(super) entity_id: EntityId,
    pub(super) box_index: usize,
}

#[derive(Clone, Debug)]
struct FluidBoxNode {
    key: FluidBoxKey,
    capacity_milliunits: u64,
    filter: Option<FluidId>,
    amount_milliunits: u64,
    fluid_id: Option<FluidId>,
    endpoints: Vec<FluidEndpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FluidEndpoint {
    x: i32,
    y: i32,
    axis: FluidEndpointAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum FluidEndpointAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
struct BuiltFluidNetwork {
    network_id: u32,
    boxes: Vec<FluidBoxKey>,
    capacity_milliunits: u64,
    total_milliunits: u64,
    fluid_id: Option<FluidId>,
    blocked: bool,
}

#[derive(Clone)]
struct UnionFind {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl Simulation {
    pub(super) fn invalidate_fluid_state(&mut self) {
        self.fluid_networks.clear();
    }

    pub(super) fn advance_fluids_before_power(&mut self) {
        self.rebuild_fluid_networks_and_equalize();
        self.advance_offshore_pumps();
        self.rebuild_fluid_networks_and_equalize();
        self.advance_boilers();
        self.rebuild_fluid_networks_and_equalize();
    }

    pub(super) fn rebuild_fluid_networks_and_equalize(&mut self) {
        let networks = self.build_fluid_networks();
        for network in &networks {
            if !network.blocked {
                self.equalize_fluid_network(network);
            }
        }

        self.fluid_networks = networks
            .iter()
            .map(|network| self.fluid_network_snapshot(network))
            .collect();
    }

    pub(super) fn fluid_network_id_for_box_key(&self, key: FluidBoxKey) -> Option<u32> {
        self.fluid_networks
            .iter()
            .find(|network| {
                network.boxes.iter().any(|box_snapshot| {
                    box_snapshot.entity_id == key.entity_id
                        && box_snapshot.box_index == key.box_index
                })
            })
            .map(|network| network.network_id)
    }

    pub(super) fn fluid_network_total_for_fluid(&self, network_id: u32, fluid_id: FluidId) -> u64 {
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

    pub(super) fn fluid_network_available_capacity_for_fluid(
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

    pub(super) fn add_fluid_to_network(
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

    pub(super) fn consume_fluid_from_network(
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
                .entities
                .get(placed.prototype_id.index())
                .filter(|prototype| prototype.id == placed.prototype_id)
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
            self.add_fluid_to_network(network_id, water, amount);
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
            let Some(entity_prototype) = self
                .world
                .prototypes
                .entities
                .get(placed.prototype_id.index())
                .filter(|prototype| prototype.id == placed.prototype_id)
            else {
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
            let added = self.add_fluid_to_network(steam_network_id, steam, steam_amount);
            debug_assert_eq!(added, steam_amount);
        }
    }

    fn build_fluid_networks(&self) -> Vec<BuiltFluidNetwork> {
        let nodes = self.fluid_box_nodes();
        if nodes.is_empty() {
            return Vec::new();
        }

        let mut union_find = UnionFind::new(nodes.len());
        let mut endpoint_boxes = BTreeMap::<FluidEndpoint, Vec<usize>>::new();
        for (index, node) in nodes.iter().enumerate() {
            for endpoint in &node.endpoints {
                endpoint_boxes.entry(*endpoint).or_default().push(index);
            }
        }

        for indices in endpoint_boxes.values() {
            let Some((&first, rest)) = indices.split_first() else {
                continue;
            };
            for index in rest {
                union_find.union(first, *index);
            }
        }

        let mut components = BTreeMap::<usize, Vec<usize>>::new();
        for index in 0..nodes.len() {
            let root = union_find.find(index);
            components.entry(root).or_default().push(index);
        }

        let mut components_by_min_key = BTreeMap::<FluidBoxKey, Vec<usize>>::new();
        for indices in components.into_values() {
            let min_key = indices
                .iter()
                .map(|index| nodes[*index].key)
                .min()
                .expect("component should contain at least one fluid box");
            components_by_min_key.insert(min_key, indices);
        }

        components_by_min_key
            .into_values()
            .enumerate()
            .map(|(network_id, mut indices)| {
                indices.sort_by_key(|index| nodes[*index].key);
                built_fluid_network(network_id as u32, &nodes, &indices)
            })
            .collect()
    }

    fn fluid_box_nodes(&self) -> Vec<FluidBoxNode> {
        let mut nodes = Vec::new();
        for placed in self.entities.placed_entities.values() {
            let Some(entity_fluid_boxes) = self.entities.fluid_boxes.get(&placed.id) else {
                continue;
            };
            let Some(prototype) = self
                .world
                .prototypes
                .entities
                .get(placed.prototype_id.index())
                .filter(|prototype| prototype.id == placed.prototype_id)
            else {
                continue;
            };

            for (box_index, fluid_box) in prototype.fluid_boxes.iter().enumerate() {
                let Some(state) = entity_fluid_boxes.get(box_index) else {
                    continue;
                };
                let endpoints = fluid_box
                    .connections
                    .iter()
                    .filter_map(|connection| rotated_fluid_endpoint(placed, prototype, connection))
                    .collect();
                nodes.push(FluidBoxNode {
                    key: FluidBoxKey {
                        entity_id: placed.id,
                        box_index,
                    },
                    capacity_milliunits: fluid_box.capacity_milliunits,
                    filter: fluid_box.filter,
                    amount_milliunits: state.amount_milliunits,
                    fluid_id: state.fluid_id,
                    endpoints,
                });
            }
        }
        nodes
    }

    fn equalize_fluid_network(&mut self, network: &BuiltFluidNetwork) {
        if network.boxes.is_empty() || network.capacity_milliunits == 0 {
            return;
        }

        let fluid_id = network.fluid_id;
        if network.total_milliunits == 0 {
            for key in &network.boxes {
                if let Some(state) = self
                    .entities
                    .fluid_boxes
                    .get_mut(&key.entity_id)
                    .and_then(|boxes| boxes.get_mut(key.box_index))
                {
                    state.amount_milliunits = 0;
                    state.fluid_id = None;
                }
            }
            return;
        }
        let Some(fluid_id) = fluid_id else {
            return;
        };

        let mut assignments = Vec::with_capacity(network.boxes.len());
        let mut assigned_total = 0_u64;
        for key in &network.boxes {
            let Some(capacity) = self.fluid_box_capacity(*key) else {
                continue;
            };
            let assigned = proportional_amount(
                network.total_milliunits,
                capacity,
                network.capacity_milliunits,
            );
            assignments.push((*key, capacity, assigned));
            assigned_total = assigned_total.saturating_add(assigned);
        }

        let mut remainder = network.total_milliunits.saturating_sub(assigned_total);
        for (_, capacity, assigned) in &mut assignments {
            if remainder == 0 {
                break;
            }
            if *assigned < *capacity {
                *assigned += 1;
                remainder -= 1;
            }
        }

        debug_assert_eq!(remainder, 0);
        for (key, _, assigned) in assignments {
            if let Some(state) = self
                .entities
                .fluid_boxes
                .get_mut(&key.entity_id)
                .and_then(|boxes| boxes.get_mut(key.box_index))
            {
                state.amount_milliunits = assigned;
                state.fluid_id = (assigned > 0).then_some(fluid_id);
            }
        }
    }

    fn fluid_box_capacity(&self, key: FluidBoxKey) -> Option<u64> {
        let placed = self.entities.placed_entity(key.entity_id)?;
        self.world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
            .filter(|prototype| prototype.id == placed.prototype_id)?
            .fluid_boxes
            .get(key.box_index)
            .map(|fluid_box| fluid_box.capacity_milliunits)
    }

    fn fluid_network_snapshot(&self, network: &BuiltFluidNetwork) -> FluidNetworkSnapshot {
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

    fn fluid_network_box_snapshot(&self, key: FluidBoxKey) -> Option<FluidNetworkBoxSnapshot> {
        let placed = self.entities.placed_entity(key.entity_id)?;
        let prototype = self
            .world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
            .filter(|prototype| prototype.id == placed.prototype_id)?;
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

    fn fluid_network_by_id(&self, network_id: u32) -> Option<&FluidNetworkSnapshot> {
        self.fluid_networks
            .get(network_id as usize)
            .filter(|network| network.network_id == network_id)
    }
}

pub(super) fn per_tick_milliunits(per_second_milliunits: u64) -> u64 {
    ceil_div_u64(per_second_milliunits, FIXED_SIM_TICKS_PER_SECOND_F64 as u64)
}

pub(super) fn ceil_div_u64(numerator: u64, denominator: u64) -> u64 {
    if numerator == 0 {
        0
    } else {
        numerator.div_ceil(denominator)
    }
}

fn built_fluid_network(
    network_id: u32,
    nodes: &[FluidBoxNode],
    indices: &[usize],
) -> BuiltFluidNetwork {
    let mut filters = BTreeSet::new();
    let mut nonempty_fluids = BTreeSet::new();
    let mut boxes = Vec::with_capacity(indices.len());
    let mut capacity_milliunits = 0_u64;
    let mut total_milliunits = 0_u64;

    for index in indices {
        let node = &nodes[*index];
        boxes.push(node.key);
        capacity_milliunits = capacity_milliunits.saturating_add(node.capacity_milliunits);
        total_milliunits = total_milliunits.saturating_add(node.amount_milliunits);
        if let Some(filter) = node.filter {
            filters.insert(filter);
        }
        if node.amount_milliunits > 0
            && let Some(fluid_id) = node.fluid_id
        {
            nonempty_fluids.insert(fluid_id);
        }
    }

    let filter_fluid = single_fluid(filters.iter().copied());
    let nonempty_fluid = single_fluid(nonempty_fluids.iter().copied());
    let blocked = filters.len() > 1
        || nonempty_fluids.len() > 1
        || filter_fluid
            .zip(nonempty_fluid)
            .is_some_and(|(filter, fluid)| filter != fluid);
    let fluid_id = if nonempty_fluids.len() > 1 {
        None
    } else {
        nonempty_fluid.or(filter_fluid)
    };

    BuiltFluidNetwork {
        network_id,
        boxes,
        capacity_milliunits,
        total_milliunits,
        fluid_id,
        blocked,
    }
}

fn single_fluid(mut fluids: impl Iterator<Item = FluidId>) -> Option<FluidId> {
    let first = fluids.next()?;
    fluids.next().is_none().then_some(first)
}

fn proportional_amount(total: u64, capacity: u64, total_capacity: u64) -> u64 {
    if total_capacity == 0 {
        return 0;
    }

    ((u128::from(total) * u128::from(capacity)) / u128::from(total_capacity)) as u64
}

fn fluid_filter_accepts(filter: Option<FluidId>, fluid_id: FluidId) -> bool {
    filter.is_none_or(|filter| filter == fluid_id)
}

fn rotated_fluid_endpoint(
    placed: &PlacedEntity,
    prototype: &factory_data::EntityPrototype,
    connection: &factory_data::FluidConnectionPrototype,
) -> Option<FluidEndpoint> {
    let (local_x, local_y, side) = rotate_fluid_connection(
        connection.local_offset.x,
        connection.local_offset.y,
        connection.side,
        prototype.size.x,
        prototype.size.y,
        placed.direction,
    )?;
    let tile_x = placed.footprint.x + local_x;
    let tile_y = placed.footprint.y + local_y;

    Some(endpoint_for_side(tile_x, tile_y, side))
}

fn rotate_fluid_connection(
    local_x: i32,
    local_y: i32,
    side: factory_data::FluidConnectionSide,
    width: i32,
    height: i32,
    direction: Direction,
) -> Option<(i32, i32, factory_data::FluidConnectionSide)> {
    if local_x < 0 || local_y < 0 || local_x >= width || local_y >= height {
        return None;
    }

    match direction {
        Direction::North => Some((local_x, local_y, side)),
        Direction::East => Some((height - 1 - local_y, local_x, rotate_side_clockwise(side))),
        Direction::South => Some((
            width - 1 - local_x,
            height - 1 - local_y,
            opposite_side(side),
        )),
        Direction::West => Some((
            local_y,
            width - 1 - local_x,
            rotate_side_counter_clockwise(side),
        )),
    }
}

fn endpoint_for_side(
    tile_x: i32,
    tile_y: i32,
    side: factory_data::FluidConnectionSide,
) -> FluidEndpoint {
    match side {
        factory_data::FluidConnectionSide::North => FluidEndpoint {
            x: tile_x,
            y: tile_y,
            axis: FluidEndpointAxis::Horizontal,
        },
        factory_data::FluidConnectionSide::East => FluidEndpoint {
            x: tile_x + 1,
            y: tile_y,
            axis: FluidEndpointAxis::Vertical,
        },
        factory_data::FluidConnectionSide::South => FluidEndpoint {
            x: tile_x,
            y: tile_y + 1,
            axis: FluidEndpointAxis::Horizontal,
        },
        factory_data::FluidConnectionSide::West => FluidEndpoint {
            x: tile_x,
            y: tile_y,
            axis: FluidEndpointAxis::Vertical,
        },
    }
}

fn rotate_side_clockwise(
    side: factory_data::FluidConnectionSide,
) -> factory_data::FluidConnectionSide {
    match side {
        factory_data::FluidConnectionSide::North => factory_data::FluidConnectionSide::East,
        factory_data::FluidConnectionSide::East => factory_data::FluidConnectionSide::South,
        factory_data::FluidConnectionSide::South => factory_data::FluidConnectionSide::West,
        factory_data::FluidConnectionSide::West => factory_data::FluidConnectionSide::North,
    }
}

fn rotate_side_counter_clockwise(
    side: factory_data::FluidConnectionSide,
) -> factory_data::FluidConnectionSide {
    match side {
        factory_data::FluidConnectionSide::North => factory_data::FluidConnectionSide::West,
        factory_data::FluidConnectionSide::West => factory_data::FluidConnectionSide::South,
        factory_data::FluidConnectionSide::South => factory_data::FluidConnectionSide::East,
        factory_data::FluidConnectionSide::East => factory_data::FluidConnectionSide::North,
    }
}

fn opposite_side(side: factory_data::FluidConnectionSide) -> factory_data::FluidConnectionSide {
    match side {
        factory_data::FluidConnectionSide::North => factory_data::FluidConnectionSide::South,
        factory_data::FluidConnectionSide::East => factory_data::FluidConnectionSide::West,
        factory_data::FluidConnectionSide::South => factory_data::FluidConnectionSide::North,
        factory_data::FluidConnectionSide::West => factory_data::FluidConnectionSide::East,
    }
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parents: (0..size).collect(),
            ranks: vec![0; size],
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parents[index] != index {
            self.parents[index] = self.find(self.parents[index]);
        }
        self.parents[index]
    }

    fn union(&mut self, first: usize, second: usize) {
        let first_root = self.find(first);
        let second_root = self.find(second);
        if first_root == second_root {
            return;
        }

        match self.ranks[first_root].cmp(&self.ranks[second_root]) {
            std::cmp::Ordering::Less => self.parents[first_root] = second_root,
            std::cmp::Ordering::Greater => self.parents[second_root] = first_root,
            std::cmp::Ordering::Equal => {
                self.parents[second_root] = first_root;
                self.ranks[first_root] += 1;
            }
        }
    }
}
