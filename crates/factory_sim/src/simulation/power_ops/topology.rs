use std::collections::BTreeMap;

use crate::simulation::disjoint_set::DisjointSet;

use super::types::{NetworkAccumulator, PoleNode};
use super::*;

impl Simulation {
    pub(super) fn rebuild_power_topology(&self) -> PowerTopologyCache {
        let poles = self.pole_nodes();
        if poles.is_empty() {
            return PowerTopologyCache::default();
        }

        let mut disjoint_set = DisjointSet::new(poles.len());
        connect_poles_within_wire_reach(&poles, &mut disjoint_set);

        let mut min_entity_by_root = BTreeMap::<usize, EntityId>::new();
        for (index, pole) in poles.iter().enumerate() {
            let root = disjoint_set.find(index);
            min_entity_by_root
                .entry(root)
                .and_modify(|min_entity| *min_entity = (*min_entity).min(pole.entity_id))
                .or_insert(pole.entity_id);
            debug_assert_eq!(pole.entity_id, poles[index].entity_id);
        }

        let roots_by_min_entity = min_entity_by_root
            .into_iter()
            .map(|(root, min_entity)| (min_entity, root))
            .collect::<BTreeMap<_, _>>();
        let root_network_ids = roots_by_min_entity
            .values()
            .enumerate()
            .map(|(network_id, root)| (*root, network_id as u32))
            .collect::<BTreeMap<_, _>>();
        let mut pole_counts = vec![0; root_network_ids.len()];
        let mut coverage = BTreeMap::<(i32, i32), u32>::new();

        for (index, pole) in poles.iter().enumerate() {
            let root = disjoint_set.find(index);
            let network_id = root_network_ids[&root];
            pole_counts[network_id as usize] += 1;

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

        PowerTopologyCache {
            network_ids_by_entity,
            pole_counts,
        }
    }

    pub(super) fn pole_nodes(&self) -> Vec<PoleNode<'_>> {
        self.entities
            .electric_poles
            .keys()
            .filter_map(|entity_id| {
                let placed = self.entities.placed_entity(*entity_id)?;
                let prototype = self
                    .world
                    .prototypes
                    .entity(placed.prototype_id)?
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
}

impl PowerTopologyCache {
    pub(super) fn network_accumulators(&self) -> Vec<NetworkAccumulator> {
        self.pole_counts
            .iter()
            .map(|pole_count| NetworkAccumulator {
                pole_count: *pole_count,
                satisfaction_permyriad: POWER_SATISFACTION_FULL_PERMYRIAD,
                ..NetworkAccumulator::default()
            })
            .collect()
    }
}

pub(super) fn connect_poles_within_wire_reach(
    poles: &[PoleNode<'_>],
    disjoint_set: &mut DisjointSet,
) {
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
                        disjoint_set.union(index, *candidate_index);
                    }
                }
            }
        }
    }
}

pub(super) fn poles_are_within_mutual_reach(first: &PoleNode<'_>, second: &PoleNode<'_>) -> bool {
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

pub(super) fn footprint_center_x2(footprint: &EntityFootprint) -> (i32, i32) {
    (
        footprint.x.saturating_mul(2) + footprint.width,
        footprint.y.saturating_mul(2) + footprint.height,
    )
}

pub(super) fn pole_supply_tiles(
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
