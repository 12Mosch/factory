use crate::robots::{EntityRoboportStatus, RobotNetworkRoboportSnapshot, TileBounds};
use crate::simulation::*;
use factory_data::RobotKind;

use super::types::RobotNetworkTopology;

impl Simulation {
    /// Network `entity_id` belongs to, or `None` when it is not a roboport.
    ///
    /// Answered from the settled topology, so between a placement and the robot
    /// pass that follows it this reports `None` rather than a stale network id.
    /// Presentation reads this every frame and would otherwise draw a network
    /// that no longer exists; simulation callers run after
    /// [`Simulation::ensure_robot_network_topology`] and always see the rebuilt
    /// answer.
    pub fn robot_network_id_for_entity(&self, entity_id: EntityId) -> Option<u32> {
        self.robots.network_ids_by_entity.get(&entity_id).copied()
    }

    /// Settled robot networks, one snapshot per network.
    pub fn robot_networks(&self) -> &[RobotNetworkSnapshot] {
        &self.robots.networks
    }

    /// The network whose construction coverage contains `(x, y)`, or `None`
    /// when no roboport reaches the tile.
    ///
    /// Coverage is the union of the member construction squares, so this walks
    /// the squares rather than testing the network bounding box: an L-shaped
    /// network must not claim the empty corner of its bounding box. Ties go to
    /// the lowest network id so the answer never depends on iteration order.
    ///
    /// Like [`Simulation::robot_network_id_for_entity`], this reads the settled
    /// topology and reports nothing while a rebuild is pending.
    pub fn construction_network_covering_tile(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<u32> {
        self.robots
            .topology_networks
            .iter()
            .find(|network| {
                network.construction_bounds.contains(x, y)
                    && network
                        .roboports
                        .iter()
                        .any(|roboport| roboport.construction_bounds.contains(x, y))
            })
            .map(|network| network.network_id)
    }

    /// The network whose *logistic* coverage contains `(x, y)`, or `None` when
    /// no roboport reaches the tile.
    ///
    /// The logistic twin of [`Simulation::construction_network_covering_tile`],
    /// and separate from it on purpose: the two radii differ, so a chest can sit
    /// inside a network's construction reach and still be outside the logistic
    /// reach that would let robots service it.
    pub fn logistic_network_covering_tile(
        &self,
        x: WorldTileCoord,
        y: WorldTileCoord,
    ) -> Option<u32> {
        self.robots
            .topology_networks
            .iter()
            .find(|network| {
                network.logistic_bounds.contains(x, y)
                    && network
                        .roboports
                        .iter()
                        .any(|roboport| roboport.logistic_bounds.contains(x, y))
            })
            .map(|network| network.network_id)
    }

    /// The logistic network covering an entity, measured from its footprint
    /// center — the same reference point construction jobs use, so a wide
    /// entity is never half in and half out.
    pub fn logistic_network_covering_entity(&self, entity_id: EntityId) -> Option<u32> {
        let footprint = self.entities.placed_entity(entity_id)?.footprint;
        self.logistic_network_covering_tile(
            footprint.x + i64::from(footprint.width.saturating_sub(1)) / 2,
            footprint.y + i64::from(footprint.height.saturating_sub(1)) / 2,
        )
    }

    /// Logistic contents of one network, keyed by item.
    ///
    /// Answered from the incrementally maintained index, so this is a map
    /// lookup rather than a walk over the network's chests.
    pub fn logistic_network_contents(
        &self,
        network_id: u32,
    ) -> Option<&std::collections::BTreeMap<ItemId, crate::robots::LogisticItemTotals>> {
        self.robots.logistic.contents(network_id)
    }

    /// What one network holds and wants of a single item.
    pub fn logistic_network_item_totals(
        &self,
        network_id: u32,
        item_id: ItemId,
    ) -> crate::robots::LogisticItemTotals {
        self.logistic_network_contents(network_id)
            .and_then(|contents| contents.get(&item_id).copied())
            .unwrap_or_default()
    }

    /// Network a logistic chest is currently counted into.
    ///
    /// Read from the index rather than recomputed, so it reports what the
    /// totals were actually built from — including `None` for a chest that has
    /// been placed but not yet indexed.
    pub fn logistic_network_id_for_chest(&self, entity_id: EntityId) -> Option<u32> {
        if !self.entities.logistic_chests.contains_key(&entity_id) {
            return None;
        }
        self.robots.logistic.network_of(entity_id)
    }

    /// Construction and logistic squares of one roboport, in world tiles.
    ///
    /// Answered straight from the prototype rather than from the topology cache
    /// so the build preview can show the coverage of a roboport that is only
    /// being *considered*, not yet placed.
    pub fn roboport_coverage_bounds_for_footprint(
        &self,
        prototype_id: EntityPrototypeId,
        footprint: EntityFootprint,
    ) -> Option<(TileBounds, TileBounds)> {
        let roboport = self.world.prototypes.entity(prototype_id)?.roboport?;
        Some((
            coverage_bounds(footprint, roboport.construction_radius_tiles),
            coverage_bounds(footprint, roboport.logistic_radius_tiles),
        ))
    }

    /// Robot-network status of one entity, or `None` when it is not a roboport.
    pub fn entity_roboport_status(&self, entity_id: EntityId) -> Option<EntityRoboportStatus> {
        let state = self.entities.roboports.get(&entity_id)?;
        let placed = self.entities.placed_entity(entity_id)?;
        let (construction_bounds, logistic_bounds) =
            self.roboport_coverage_bounds_for_footprint(placed.prototype_id, placed.footprint)?;
        let network = self
            .robot_network_id_for_entity(entity_id)
            .and_then(|network_id| self.robots.networks.get(network_id as usize));
        Some(EntityRoboportStatus {
            network_id: self.robot_network_id_for_entity(entity_id),
            charge_energy_joules: state.charge_energy_joules,
            charge_capacity_joules: self.roboport_charge_capacity_joules(entity_id),
            construction_bounds,
            logistic_bounds,
            available_construction_robots: network
                .map_or(0, |network| network.available_construction_robots),
            total_construction_robots: network
                .map_or(0, |network| network.total_construction_robots),
            available_logistic_robots: network
                .map_or(0, |network| network.available_logistic_robots),
            total_logistic_robots: network.map_or(0, |network| network.total_logistic_robots),
            jobs: network.map_or_else(Default::default, |network| network.jobs),
            active_deliveries: network.map_or(0, |network| network.active_deliveries),
        })
    }

    pub(in crate::simulation) fn roboport_prototype(
        &self,
        entity_id: EntityId,
    ) -> Option<factory_data::RoboportPrototype> {
        let placed = self.entities.placed_entity(entity_id)?;
        self.world.prototypes.entity(placed.prototype_id)?.roboport
    }

    pub(in crate::simulation) fn roboport_charge_capacity_joules(
        &self,
        entity_id: EntityId,
    ) -> u64 {
        self.roboport_prototype(entity_id)
            .map_or(0, |roboport| roboport.charging_energy_buffer_joules)
    }

    #[cfg(test)]
    pub(in crate::simulation) fn robot_topology_rebuild_count(&self) -> u64 {
        self.robots.topology_rebuilds
    }

    pub(in crate::simulation) fn refresh_robot_network_work_counts(&mut self) {
        let counts = robot_network_cached_work_counts(self);

        for (snapshot, counts) in self.robots.networks.iter_mut().zip(counts) {
            snapshot.available_construction_robots = counts.available_construction_robots;
            snapshot.total_construction_robots = counts.total_construction_robots;
            snapshot.available_logistic_robots = counts.available_logistic_robots;
            snapshot.total_logistic_robots = counts.total_logistic_robots;
            snapshot.jobs = counts.jobs;
            snapshot.active_deliveries = counts.active_deliveries;
        }
    }
}

/// What one network's robots are doing, as the durable snapshot reports it.
///
/// "Available" counts robots stationed in the network's roboports and "total"
/// adds the ones it currently has in the air, so the pair reads as "idle of
/// owned" rather than as two unrelated numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::simulation) struct RobotNetworkWorkCounts {
    pub(in crate::simulation) available_construction_robots: u32,
    pub(in crate::simulation) total_construction_robots: u32,
    pub(in crate::simulation) available_logistic_robots: u32,
    pub(in crate::simulation) total_logistic_robots: u32,
    pub(in crate::simulation) jobs: crate::robots::RobotNetworkJobCounts,
    pub(in crate::simulation) active_deliveries: u32,
}

pub(in crate::simulation) fn robot_network_work_counts(
    sim: &Simulation,
) -> Vec<RobotNetworkWorkCounts> {
    let mut jobs =
        vec![crate::robots::RobotNetworkJobCounts::default(); sim.robots.topology_networks.len()];
    for job in sim
        .construction
        .queue
        .iter()
        .chain(sim.construction.reservations.keys())
    {
        let Some((x, y)) = sim.construction_job_target_tile(*job) else {
            continue;
        };
        let Some(network_id) = sim.construction_network_covering_tile(x, y) else {
            continue;
        };
        jobs[network_id as usize].add(*job);
    }
    robot_network_work_counts_with_jobs(sim, jobs)
}

fn robot_network_cached_work_counts(sim: &Simulation) -> Vec<RobotNetworkWorkCounts> {
    let jobs = if sim.robots.job_counts_by_network.len() == sim.robots.topology_networks.len() {
        sim.robots.job_counts_by_network.clone()
    } else {
        vec![crate::robots::RobotNetworkJobCounts::default(); sim.robots.topology_networks.len()]
    };
    robot_network_work_counts_with_jobs(sim, jobs)
}

fn robot_network_work_counts_with_jobs(
    sim: &Simulation,
    jobs: Vec<crate::robots::RobotNetworkJobCounts>,
) -> Vec<RobotNetworkWorkCounts> {
    let mut counts = jobs
        .into_iter()
        .map(|jobs| RobotNetworkWorkCounts {
            jobs,
            ..RobotNetworkWorkCounts::default()
        })
        .collect::<Vec<_>>();

    for (network_index, network) in sim.robots.topology_networks.iter().enumerate() {
        for node in &network.roboports {
            let Some(state) = sim.entities.roboports.get(&node.entity_id) else {
                continue;
            };
            for stack in state.robots.slots().iter().filter_map(|slot| slot.stack()) {
                let Some(kind) = robot_kind(sim, stack.item_id()) else {
                    continue;
                };
                let entry = &mut counts[network_index];
                let available = match kind {
                    RobotKind::Construction => &mut entry.available_construction_robots,
                    RobotKind::Logistic => &mut entry.available_logistic_robots,
                };
                *available = available.saturating_add(u32::from(stack.count()));
            }
        }
        counts[network_index].total_construction_robots =
            counts[network_index].available_construction_robots;
        counts[network_index].total_logistic_robots =
            counts[network_index].available_logistic_robots;
    }

    for robot in sim.robot_flights.robots.values() {
        let Some(network_id) = robot
            .home_roboport
            .and_then(|home| sim.robots.network_ids_by_entity.get(&home).copied())
        else {
            continue;
        };
        let entry = &mut counts[network_id as usize];
        match robot_kind(sim, robot.item_id) {
            Some(RobotKind::Construction) => {
                entry.total_construction_robots = entry.total_construction_robots.saturating_add(1);
            }
            Some(RobotKind::Logistic) => {
                entry.total_logistic_robots = entry.total_logistic_robots.saturating_add(1);
                if robot.delivery.is_some() {
                    entry.active_deliveries = entry.active_deliveries.saturating_add(1);
                }
            }
            None => {}
        }
    }

    counts
}

fn robot_kind(sim: &Simulation, item_id: ItemId) -> Option<RobotKind> {
    Some(sim.world.prototypes.item(item_id)?.robot?.kind)
}

/// Square a roboport radius covers around `footprint`, as inclusive tile
/// bounds.
pub(in crate::simulation) fn coverage_bounds(
    footprint: EntityFootprint,
    radius_tiles: u16,
) -> TileBounds {
    let (min_x, min_y, max_x, max_y) = factory_data::roboport_coverage_bounds(
        footprint.x,
        footprint.y,
        footprint.width,
        footprint.height,
        radius_tiles,
    );
    TileBounds {
        min_x,
        min_y,
        max_x,
        max_y,
    }
}

pub(super) fn update_robot_network_snapshot(
    entities: &EntityStore,
    network: &RobotNetworkTopology,
    snapshot: &mut RobotNetworkSnapshot,
) {
    snapshot.network_id = network.network_id;
    snapshot.construction_bounds = network.construction_bounds;
    snapshot.logistic_bounds = network.logistic_bounds;
    snapshot.charge_capacity_joules = network.charge_capacity_joules;

    let mut charge_energy_joules = 0_u64;
    let mut snapshot_index = 0;
    for roboport in &network.roboports {
        let Some(state) = entities.roboports.get(&roboport.entity_id) else {
            continue;
        };
        charge_energy_joules = charge_energy_joules.saturating_add(state.charge_energy_joules);
        let roboport_snapshot = RobotNetworkRoboportSnapshot {
            entity_id: roboport.entity_id,
            construction_bounds: roboport.construction_bounds,
            logistic_bounds: roboport.logistic_bounds,
            charge_energy_joules: state.charge_energy_joules,
            charge_capacity_joules: roboport.charge_capacity_joules,
        };
        if let Some(existing) = snapshot.roboports.get_mut(snapshot_index) {
            *existing = roboport_snapshot;
        } else {
            snapshot.roboports.push(roboport_snapshot);
        }
        snapshot_index += 1;
    }
    snapshot.roboports.truncate(snapshot_index);
    snapshot.charge_energy_joules = charge_energy_joules;
}
