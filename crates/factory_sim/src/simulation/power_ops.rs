use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct PoleNode<'a> {
    entity_id: EntityId,
    placed: &'a PlacedEntity,
    prototype: &'a factory_data::ElectricPolePrototype,
    center_x2: i32,
    center_y2: i32,
}

#[derive(Clone, Copy)]
struct SteamEngineAssignment {
    network_id: u32,
    steam_network_id: u32,
    available_power_output_watts: u64,
    max_power_output_watts: u64,
    steam_budget_milliunits: u64,
    steam_consumption_per_tick_milliunits: u64,
}

#[derive(Default)]
struct NetworkAccumulator {
    pole_count: usize,
    producer_count: usize,
    consumer_count: usize,
    available_production_watts: u64,
    consumption_watts: u64,
    production_watts: u64,
    satisfaction_permyriad: u32,
}

#[derive(Clone)]
struct UnionFind {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl Simulation {
    pub(super) fn invalidate_power_state(&mut self) {
        self.power_summary = PowerSummary {
            satisfaction_permyriad: POWER_SATISFACTION_FULL_PERMYRIAD,
            ..PowerSummary::default()
        };
        self.power_networks.clear();
        self.entity_power_statuses.clear();
    }

    pub(super) fn rebuild_power_state(&mut self) {
        let (network_ids_by_entity, mut networks) = self.build_pole_networks();
        let consumer_demands = self.consumer_power_demands();

        for (entity_id, (active_usage_watts, drain_watts)) in &consumer_demands {
            let Some(network_id) = network_ids_by_entity.get(entity_id).copied() else {
                continue;
            };
            let network = &mut networks[network_id as usize];
            network.consumer_count += 1;
            network.consumption_watts = network
                .consumption_watts
                .saturating_add(active_usage_watts.saturating_add(*drain_watts));
        }

        let engine_assignments =
            self.assign_steam_engines_to_fluid_networks(&network_ids_by_entity, &networks);
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
        self.rebuild_fluid_networks_and_equalize();
        self.power_networks = network_snapshots(&networks);
        self.power_summary = aggregate_power_summary(&self.power_networks);
        self.entity_power_statuses =
            self.consumer_power_statuses(network_ids_by_entity, consumer_demands);
    }

    fn build_pole_networks(&self) -> (BTreeMap<EntityId, u32>, Vec<NetworkAccumulator>) {
        let poles = self.pole_nodes();
        if poles.is_empty() {
            return (BTreeMap::new(), Vec::new());
        }

        let mut union_find = UnionFind::new(poles.len());
        connect_poles_within_wire_reach(&poles, &mut union_find);

        let mut roots_by_min_entity = BTreeMap::<EntityId, usize>::new();
        for (index, pole) in poles.iter().enumerate() {
            let root = union_find.find(index);
            roots_by_min_entity
                .entry(component_min_entity_id(root, &poles, &mut union_find))
                .or_insert(root);
            debug_assert_eq!(pole.entity_id, poles[index].entity_id);
        }

        let root_network_ids = roots_by_min_entity
            .values()
            .enumerate()
            .map(|(network_id, root)| (*root, network_id as u32))
            .collect::<BTreeMap<_, _>>();
        let mut networks = (0..root_network_ids.len())
            .map(|_| NetworkAccumulator {
                satisfaction_permyriad: POWER_SATISFACTION_FULL_PERMYRIAD,
                ..NetworkAccumulator::default()
            })
            .collect::<Vec<_>>();
        let mut coverage = BTreeMap::<(i32, i32), u32>::new();

        for (index, pole) in poles.iter().enumerate() {
            let root = union_find.find(index);
            let network_id = root_network_ids[&root];
            networks[network_id as usize].pole_count += 1;

            for tile in pole_supply_tiles(pole.placed, pole.prototype) {
                coverage
                    .entry(tile)
                    .and_modify(|existing| *existing = (*existing).min(network_id))
                    .or_insert(network_id);
            }
        }

        let mut network_ids_by_entity = BTreeMap::new();
        for placed in self.entities.placed_entities.values() {
            let covered_network = placed
                .footprint
                .tiles()
                .into_iter()
                .filter_map(|tile| coverage.get(&tile).copied())
                .min();
            if let Some(network_id) = covered_network {
                network_ids_by_entity.insert(placed.id, network_id);
            }
        }

        (network_ids_by_entity, networks)
    }

    fn pole_nodes(&self) -> Vec<PoleNode<'_>> {
        self.entities
            .electric_poles
            .keys()
            .filter_map(|entity_id| {
                let placed = self.entities.placed_entity(*entity_id)?;
                let prototype = self
                    .world
                    .prototypes
                    .entities
                    .get(placed.prototype_id.index())
                    .filter(|prototype| prototype.id == placed.prototype_id)?
                    .electric_pole
                    .as_ref()?;
                let (center_x2, center_y2) = footprint_center_x2(&placed.footprint);
                Some(PoleNode {
                    entity_id: *entity_id,
                    placed,
                    prototype,
                    center_x2,
                    center_y2,
                })
            })
            .collect()
    }

    fn consumer_power_demands(&self) -> BTreeMap<EntityId, (u64, u64)> {
        self.entities
            .electric_consumers
            .keys()
            .filter_map(|entity_id| {
                let placed = self.entities.placed_entity(*entity_id)?;
                let energy_source = self
                    .world
                    .prototypes
                    .entities
                    .get(placed.prototype_id.index())
                    .filter(|prototype| prototype.id == placed.prototype_id)?
                    .electric_energy_source
                    .as_ref()?;
                let active_usage_watts = if self.electric_consumer_can_work(*entity_id) {
                    energy_source.energy_usage_watts
                } else {
                    0
                };
                Some((*entity_id, (active_usage_watts, energy_source.drain_watts)))
            })
            .collect()
    }

    fn consumer_power_statuses(
        &self,
        network_ids_by_entity: BTreeMap<EntityId, u32>,
        consumer_demands: BTreeMap<EntityId, (u64, u64)>,
    ) -> BTreeMap<EntityId, EntityPowerStatus> {
        consumer_demands
            .into_iter()
            .map(|(entity_id, (active_usage_watts, drain_watts))| {
                let network_id = network_ids_by_entity.get(&entity_id).copied();
                let satisfaction_permyriad = network_id
                    .and_then(|network_id| self.power_networks.get(network_id as usize))
                    .map(|network| network.satisfaction_permyriad)
                    .unwrap_or(0);
                (
                    entity_id,
                    EntityPowerStatus {
                        network_id,
                        satisfaction_permyriad,
                        active_usage_watts,
                        drain_watts,
                    },
                )
            })
            .collect()
    }

    fn electric_consumer_can_work(&self, entity_id: EntityId) -> bool {
        if let Ok(state) = self.entities.assembler_state(entity_id) {
            return self.assembler_can_work(state);
        }
        if let Ok(state) = self.entities.lab_state(entity_id) {
            return self.lab_can_work(state);
        }
        if let (Some(placed), Ok(state)) = (
            self.entities.placed_entity(entity_id),
            self.entities.inserter_state(entity_id),
        ) {
            return self.inserter_can_work(placed, state);
        }

        false
    }

    fn assembler_can_work(&self, state: &AssemblingMachineState) -> bool {
        let Some(recipe) = selected_assembler_recipe(&self.world.prototypes, &self.research, state)
        else {
            return false;
        };

        assembler_has_ingredients(&state.input_inventory, &recipe.ingredients)
            && assembler_output_can_accept(
                &self.world.prototypes,
                &state.output_inventory,
                &recipe.products,
            )
    }

    fn lab_can_work(&self, state: &LabState) -> bool {
        let Some(technology_id) = state.active_technology.or(self.research.active) else {
            return false;
        };
        let Some(technology) = self
            .world
            .prototypes
            .technologies
            .get(technology_id.index())
            .filter(|technology| technology.id == technology_id)
        else {
            return false;
        };

        lab_has_science_packs(&state.inventory, &technology.science_packs)
    }

    fn inserter_can_work(&self, placed: &PlacedEntity, state: &InserterState) -> bool {
        let Some(prototype) = self
            .world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
            .filter(|prototype| prototype.id == placed.prototype_id)
        else {
            return false;
        };
        let Some(inserter) = prototype.inserter.as_ref() else {
            return false;
        };
        let (pickup_tile, drop_tile) = inserter_transfer_tiles_for_prototype(placed, inserter);

        match *state {
            InserterState::WaitingForItem => {
                let Some(item_id) = peek_inserter_source_item(&self.entities, pickup_tile) else {
                    return false;
                };
                inserter_target_can_accept(
                    &self.world.prototypes,
                    &self.research,
                    &self.entities,
                    drop_tile,
                    ItemStack { item_id, count: 1 },
                )
            }
            InserterState::Picking { .. } | InserterState::Dropping { .. } => true,
            InserterState::Holding { item } => inserter_target_can_accept(
                &self.world.prototypes,
                &self.research,
                &self.entities,
                drop_tile,
                item,
            ),
        }
    }

    fn assign_steam_engines_to_fluid_networks(
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
            .fluid_networks
            .iter()
            .filter(|network| !network.blocked && network.fluid_id == Some(steam))
            .map(|network| (network.network_id, network.total_milliunits))
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

    fn steam_engine_prototype(
        &self,
        engine_id: EntityId,
    ) -> Option<&factory_data::SteamEnginePrototype> {
        let placed = self.entities.placed_entity(engine_id)?;
        self.world
            .prototypes
            .entities
            .get(placed.prototype_id.index())
            .filter(|prototype| prototype.id == placed.prototype_id)?
            .steam_engine
            .as_ref()
    }

    fn consume_steam_for_engine_output(
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
            if steam_to_consume > 0 {
                self.consume_fluid_from_network(
                    assignment.steam_network_id,
                    steam,
                    steam_to_consume,
                );
            }
        }
    }

    pub(super) fn electric_work_allowed(&mut self, entity_id: EntityId) -> bool {
        let satisfaction_permyriad = self
            .entity_power_statuses
            .get(&entity_id)
            .map(|status| status.satisfaction_permyriad)
            .unwrap_or(0);
        if satisfaction_permyriad == 0 {
            return false;
        }

        let Some(state) = self.entities.electric_consumers.get_mut(&entity_id) else {
            return true;
        };
        if satisfaction_permyriad >= POWER_SATISFACTION_FULL_PERMYRIAD {
            state.work_remainder_permyriad = 0;
            return true;
        }

        state.work_remainder_permyriad = state
            .work_remainder_permyriad
            .saturating_add(satisfaction_permyriad);
        if state.work_remainder_permyriad >= POWER_SATISFACTION_FULL_PERMYRIAD {
            state.work_remainder_permyriad -= POWER_SATISFACTION_FULL_PERMYRIAD;
            true
        } else {
            false
        }
    }
}

fn connect_poles_within_wire_reach(poles: &[PoleNode<'_>], union_find: &mut UnionFind) {
    let max_reach_x2 = poles
        .iter()
        .map(|pole| i32::from(pole.prototype.wire_reach_tiles_x2))
        .max()
        .unwrap_or(1)
        .max(1);
    let bucket_span_x2 = max_reach_x2;
    let mut buckets = BTreeMap::<(i32, i32), Vec<usize>>::new();

    for (index, pole) in poles.iter().enumerate() {
        buckets
            .entry((
                pole.center_x2.div_euclid(bucket_span_x2),
                pole.center_y2.div_euclid(bucket_span_x2),
            ))
            .or_default()
            .push(index);
    }

    for (index, pole) in poles.iter().enumerate() {
        let bucket_x = pole.center_x2.div_euclid(bucket_span_x2);
        let bucket_y = pole.center_y2.div_euclid(bucket_span_x2);
        for y in bucket_y - 1..=bucket_y + 1 {
            for x in bucket_x - 1..=bucket_x + 1 {
                let Some(candidate_indices) = buckets.get(&(x, y)) else {
                    continue;
                };
                for candidate_index in candidate_indices {
                    if *candidate_index <= index {
                        continue;
                    }
                    if poles_are_within_mutual_reach(pole, &poles[*candidate_index]) {
                        union_find.union(index, *candidate_index);
                    }
                }
            }
        }
    }
}

fn poles_are_within_mutual_reach(first: &PoleNode<'_>, second: &PoleNode<'_>) -> bool {
    let reach_x2 = i64::from(
        first
            .prototype
            .wire_reach_tiles_x2
            .min(second.prototype.wire_reach_tiles_x2),
    );
    let dx = i64::from(first.center_x2 - second.center_x2);
    let dy = i64::from(first.center_y2 - second.center_y2);

    dx * dx + dy * dy <= reach_x2 * reach_x2
}

fn component_min_entity_id(
    root: usize,
    poles: &[PoleNode<'_>],
    union_find: &mut UnionFind,
) -> EntityId {
    poles
        .iter()
        .enumerate()
        .filter(|(index, _)| union_find.find(*index) == root)
        .map(|(_, pole)| pole.entity_id)
        .min()
        .expect("component root should contain at least one pole")
}

fn footprint_center_x2(footprint: &EntityFootprint) -> (i32, i32) {
    (
        footprint.x.saturating_mul(2) + footprint.width,
        footprint.y.saturating_mul(2) + footprint.height,
    )
}

fn pole_supply_tiles(
    placed: &PlacedEntity,
    prototype: &factory_data::ElectricPolePrototype,
) -> Vec<(i32, i32)> {
    let center_x = placed.footprint.x + (placed.footprint.width - 1) / 2;
    let center_y = placed.footprint.y + (placed.footprint.height - 1) / 2;
    let width = prototype.supply_area_tiles.x.max(1);
    let height = prototype.supply_area_tiles.y.max(1);
    let start_x = center_x - width / 2;
    let start_y = center_y - height / 2;
    let mut tiles = Vec::with_capacity((width * height) as usize);

    for y in start_y..start_y + height {
        for x in start_x..start_x + width {
            tiles.push((x, y));
        }
    }

    tiles
}

fn power_satisfaction(available_watts: u64, demand_watts: u64) -> (u64, u32) {
    if demand_watts == 0 {
        return (0, POWER_SATISFACTION_FULL_PERMYRIAD);
    }
    if available_watts >= demand_watts {
        return (demand_watts, POWER_SATISFACTION_FULL_PERMYRIAD);
    }

    let satisfaction =
        available_watts.saturating_mul(u64::from(POWER_SATISFACTION_FULL_PERMYRIAD)) / demand_watts;
    (available_watts, satisfaction as u32)
}

fn actual_steam_engine_outputs(
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

fn steam_consumed_for_output(
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

fn network_snapshots(networks: &[NetworkAccumulator]) -> Vec<PowerNetworkSnapshot> {
    networks
        .iter()
        .enumerate()
        .map(|(network_id, network)| PowerNetworkSnapshot {
            network_id: network_id as u32,
            pole_count: network.pole_count,
            producer_count: network.producer_count,
            consumer_count: network.consumer_count,
            production_watts: network.production_watts,
            available_production_watts: network.available_production_watts,
            consumption_watts: network.consumption_watts,
            satisfaction_permyriad: network.satisfaction_permyriad,
        })
        .collect()
}

fn aggregate_power_summary(networks: &[PowerNetworkSnapshot]) -> PowerSummary {
    let production_watts = networks
        .iter()
        .map(|network| network.production_watts)
        .sum::<u64>();
    let available_production_watts = networks
        .iter()
        .map(|network| network.available_production_watts)
        .sum::<u64>();
    let consumption_watts = networks
        .iter()
        .map(|network| network.consumption_watts)
        .sum::<u64>();
    let satisfaction_permyriad = if consumption_watts == 0 {
        POWER_SATISFACTION_FULL_PERMYRIAD
    } else {
        production_watts
            .saturating_mul(u64::from(POWER_SATISFACTION_FULL_PERMYRIAD))
            .checked_div(consumption_watts)
            .unwrap_or(u64::from(POWER_SATISFACTION_FULL_PERMYRIAD)) as u32
    };

    PowerSummary {
        production_watts,
        available_production_watts,
        consumption_watts,
        satisfaction_permyriad,
        network_count: networks.len(),
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
