//! Deterministic construction-job reconciliation, dispatch, and arrival work.

use crate::construction::ConstructionJob;
use crate::robots::{Robot, RobotActivity, RobotId};
use crate::simulation::combat_ops::repair_restore_health;
use crate::simulation::*;
use factory_data::RobotKind;

use super::flight::{roboport_dock_position, squared_distance};

pub(super) const CONSTRUCTION_JOB_EXAMINATION_BUDGET: usize = 32;

impl Simulation {
    /// Reconciles durable reservations with live targets and robots, then adds
    /// one repair job for each damaged friendly entity that has no unfinished
    /// repair work.
    pub(in crate::simulation) fn reconcile_construction_jobs(&mut self) {
        let pending = std::mem::take(&mut self.construction.queue);
        self.construction.queue = pending
            .into_iter()
            .filter(|job| self.construction_job_is_valid(*job))
            .collect();

        let reservations = self
            .construction
            .reservations
            .iter()
            .map(|(job, robot_id)| (*job, *robot_id))
            .collect::<Vec<_>>();
        for (job, robot_id) in reservations {
            let robot_matches = self
                .robot_flights
                .robots
                .get(&robot_id)
                .is_some_and(|robot| robot.construction_job == Some(job));
            let valid = self.construction_job_is_valid(job);
            let covered_by_home = robot_matches
                && self
                    .robot_flights
                    .robots
                    .get(&robot_id)
                    .is_some_and(|robot| self.job_is_covered_by_robot_home(job, robot));
            if robot_matches && valid && covered_by_home {
                continue;
            }

            self.construction.reservations.remove(&job);
            if let Some(robot) = self.robot_flights.robots.get_mut(&robot_id)
                && robot.construction_job == Some(job)
            {
                abort_robot_job(robot);
            }
            if valid {
                self.enqueue_job_once(job);
            }
        }

        let repair_targets = self
            .entities
            .entity_health
            .iter()
            .filter_map(|(entity_id, health)| {
                (health.faction == Faction::Player
                    && health.current < health.maximum
                    && !self.construction.deconstruction_marks.contains(entity_id))
                .then_some(*entity_id)
            })
            .collect::<Vec<_>>();
        for entity_id in repair_targets {
            self.enqueue_job_once(ConstructionJob::Repair(entity_id));
        }
    }

    /// Examines a bounded prefix of the queue. A temporarily blocked job moves
    /// to the tail; stale jobs are discarded; successful jobs become reserved.
    pub(in crate::simulation) fn dispatch_construction_jobs(&mut self) {
        let examinations = self
            .construction
            .queue
            .len()
            .min(CONSTRUCTION_JOB_EXAMINATION_BUDGET);
        for _ in 0..examinations {
            let Some(job) = self.construction.queue.pop_front() else {
                break;
            };
            if !self.construction_job_is_valid(job) {
                continue;
            }
            if !self.try_dispatch_construction_job(job) {
                self.construction.queue.push_back(job);
            }
        }
    }

    pub(in crate::simulation) fn resolve_construction_arrival(&mut self, robot_id: RobotId) {
        let Some(mut robot) = self.robot_flights.robots.remove(&robot_id) else {
            return;
        };
        let Some(job) = robot.construction_job else {
            self.robot_flights.robots.insert(robot_id, robot);
            return;
        };
        if self.construction.reservations.get(&job) != Some(&robot_id) {
            abort_robot_job(&mut robot);
            self.robot_flights.robots.insert(robot_id, robot);
            return;
        }

        let _completed = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.complete_robot_build(&mut robot, ghost_id)
            }
            ConstructionJob::Deconstruct(entity_id) => {
                self.complete_robot_deconstruction(&mut robot, entity_id)
            }
            ConstructionJob::Repair(entity_id) => self.complete_robot_repair(&mut robot, entity_id),
        };
        self.construction.reservations.remove(&job);
        robot.construction_job = None;
        robot.errand = None;
        if self.construction_job_is_valid(job) {
            self.enqueue_job_once(job);
        }
        if robot
            .home_roboport
            .is_some_and(|home| !self.entities.roboports.contains_key(&home))
        {
            robot.home_roboport = None;
        }
        self.robot_flights.robots.insert(robot_id, robot);
    }

    fn complete_robot_build(
        &mut self,
        robot: &mut Robot,
        ghost_id: crate::construction::GhostId,
    ) -> bool {
        let Some(ghost) = self.construction.ghosts.get(&ghost_id).cloned() else {
            move_payload_to_cargo(robot);
            return true;
        };
        let Some(payload) = robot.payload else {
            return false;
        };
        let Ok(build_item) = entity_recovery_ops::build_item_for_entity(self, ghost.prototype_id)
        else {
            move_payload_to_cargo(robot);
            return false;
        };
        if payload.item_id() != build_item || payload.count() != 1 {
            move_payload_to_cargo(robot);
            return false;
        }

        let request = placement::EntityPlacementRequest {
            prototype_id: ghost.prototype_id,
            x: ghost.x,
            y: ghost.y,
            direction: ghost.direction,
        };
        let Ok(entity_id) = placement_mutation_ops::place_entity(self, request) else {
            move_payload_to_cargo(robot);
            return false;
        };
        robot.payload = None;
        if let Some(recipe) = ghost.recipe {
            let _ = self.select_assembler_recipe(entity_id, recipe);
        }
        true
    }

    fn complete_robot_deconstruction(&mut self, robot: &mut Robot, entity_id: EntityId) -> bool {
        if !self.construction.deconstruction_marks.contains(&entity_id) {
            return true;
        }
        let Some(placed) = self.entities.placed_entity(entity_id).cloned() else {
            return true;
        };
        let Ok(stacks) = entity_recovery_ops::entity_recovery_stacks(self, &placed) else {
            return false;
        };
        if entity_mutation::remove(self, entity_id).is_none() {
            return false;
        }
        robot.cargo.extend(stacks);
        true
    }

    fn complete_robot_repair(&mut self, robot: &mut Robot, entity_id: EntityId) -> bool {
        let Some(payload) = robot.payload else {
            return false;
        };
        let Some(restore_health) = repair_restore_health(&self.world.prototypes, payload.item_id())
        else {
            move_payload_to_cargo(robot);
            return false;
        };
        if !self.repair_target_is_valid(entity_id) {
            move_payload_to_cargo(robot);
            return true;
        }

        self.restore_entity_health(entity_id, restore_health);
        robot.payload = None;
        true
    }

    fn try_dispatch_construction_job(&mut self, job: ConstructionJob) -> bool {
        let Some((target_x, target_y)) = self.construction_job_target_tile(job) else {
            return false;
        };
        let Some(network_id) = self.construction_network_covering_tile(target_x, target_y) else {
            return false;
        };
        let Some(network) = self
            .robots
            .topology_networks
            .get(network_id as usize)
            .cloned()
        else {
            return false;
        };

        if let ConstructionJob::BuildGhost(ghost_id) = job {
            let Some(ghost) = self.construction.ghosts.get(&ghost_id) else {
                return false;
            };
            if placement_validation_ops::validate_entity_placement(
                self,
                placement::EntityPlacementRequest {
                    prototype_id: ghost.prototype_id,
                    x: ghost.x,
                    y: ghost.y,
                    direction: ghost.direction,
                },
            )
            .is_err()
            {
                return false;
            }
        }

        let payload_item = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.get(&ghost_id).and_then(|ghost| {
                    entity_recovery_ops::build_item_for_entity(self, ghost.prototype_id).ok()
                })
            }
            ConstructionJob::Repair(_) => first_network_repair_item(self, &network),
            ConstructionJob::Deconstruct(_) => None,
        };
        if !matches!(job, ConstructionJob::Deconstruct(_)) && payload_item.is_none() {
            return false;
        }
        if payload_item.is_some_and(|item_id| network_material_count(self, &network, item_id) == 0)
        {
            return false;
        }

        let target_fixed = (tile_center_fixed(target_x), tile_center_fixed(target_y));
        let Some((roboport_id, robot_item, energy_capacity)) =
            dispatching_roboport(self, &network, target_fixed)
        else {
            return false;
        };
        let Some(dock) = roboport_dock_position(&self.entities, roboport_id) else {
            return false;
        };

        // All fallible checks are complete. The commit below removes exactly
        // one robot and payload item before creating the matching reservation.
        let state = self
            .entities
            .roboports
            .get_mut(&roboport_id)
            .expect("the selected roboport still exists");
        state
            .robots
            .remove(robot_item, 1)
            .expect("the selected construction robot is still stationed");
        state.charge_energy_joules -= energy_capacity;
        self.robots.mark_roboport_dirty(roboport_id);

        if let Some(item_id) = payload_item {
            withdraw_network_material(self, &network, item_id);
        }
        let payload = payload_item.map(|item_id| {
            ItemStack::new(&self.world.prototypes, item_id, 1)
                .expect("catalog material forms a one-item payload")
        });
        let id = self.robot_flights.allocate_id();
        self.robot_flights.robots.insert(
            id,
            Robot {
                id,
                item_id: robot_item,
                x: dock.0,
                y: dock.1,
                energy_joules: energy_capacity,
                home_roboport: Some(roboport_id),
                errand: Some(target_fixed),
                activity: RobotActivity::Flying,
                construction_job: Some(job),
                payload,
                cargo: Vec::new(),
            },
        );
        self.construction.reservations.insert(job, id);
        true
    }

    pub(in crate::simulation) fn construction_job_is_valid(&self, job: ConstructionJob) -> bool {
        match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.contains_key(&ghost_id)
            }
            ConstructionJob::Deconstruct(entity_id) => {
                self.construction.deconstruction_marks.contains(&entity_id)
                    && self.entities.placed_entity(entity_id).is_some()
            }
            ConstructionJob::Repair(entity_id) => self.repair_target_is_valid(entity_id),
        }
    }

    fn repair_target_is_valid(&self, entity_id: EntityId) -> bool {
        !self.construction.deconstruction_marks.contains(&entity_id)
            && self
                .entities
                .entity_health
                .get(&entity_id)
                .is_some_and(|health| {
                    health.faction == Faction::Player && health.current < health.maximum
                })
            && self.entities.placed_entity(entity_id).is_some()
    }

    pub(in crate::simulation) fn construction_job_target_tile(
        &self,
        job: ConstructionJob,
    ) -> Option<(WorldTileCoord, WorldTileCoord)> {
        let footprint = match job {
            ConstructionJob::BuildGhost(ghost_id) => {
                self.construction.ghosts.get(&ghost_id)?.footprint
            }
            ConstructionJob::Deconstruct(entity_id) | ConstructionJob::Repair(entity_id) => {
                self.entities.placed_entity(entity_id)?.footprint
            }
        };
        Some((
            footprint.x + i64::from(footprint.width.saturating_sub(1)) / 2,
            footprint.y + i64::from(footprint.height.saturating_sub(1)) / 2,
        ))
    }

    fn job_is_covered_by_robot_home(&self, job: ConstructionJob, robot: &Robot) -> bool {
        let Some(home) = robot.home_roboport else {
            return false;
        };
        let Some(home_network) = self.robots.network_ids_by_entity.get(&home).copied() else {
            return false;
        };
        let Some((x, y)) = self.construction_job_target_tile(job) else {
            return false;
        };
        self.construction_network_covering_tile(x, y) == Some(home_network)
    }

    fn enqueue_job_once(&mut self, job: ConstructionJob) {
        if !self.construction.queue.contains(&job)
            && !self.construction.reservations.contains_key(&job)
        {
            self.construction.queue.push_back(job);
        }
    }
}

pub(in crate::simulation) fn cancel_construction_job(sim: &mut Simulation, job: ConstructionJob) {
    sim.construction.queue.retain(|queued| *queued != job);
    let Some(robot_id) = sim.construction.reservations.remove(&job) else {
        return;
    };
    if let Some(robot) = sim.robot_flights.robots.get_mut(&robot_id)
        && robot.construction_job == Some(job)
    {
        abort_robot_job(robot);
    }
}

fn abort_robot_job(robot: &mut Robot) {
    move_payload_to_cargo(robot);
    robot.construction_job = None;
    robot.errand = None;
}

fn move_payload_to_cargo(robot: &mut Robot) {
    if let Some(payload) = robot.payload.take() {
        robot.cargo.push(payload);
    }
}

fn first_network_repair_item(
    sim: &Simulation,
    network: &super::types::RobotNetworkTopology,
) -> Option<ItemId> {
    network.roboports.iter().find_map(|node| {
        sim.entities
            .roboports
            .get(&node.entity_id)?
            .materials
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
            .find(|item_id| repair_restore_health(&sim.world.prototypes, *item_id).is_some())
    })
}

fn network_material_count(
    sim: &Simulation,
    network: &super::types::RobotNetworkTopology,
    item_id: ItemId,
) -> u32 {
    network
        .roboports
        .iter()
        .filter_map(|node| sim.entities.roboports.get(&node.entity_id))
        .map(|state| state.materials.count(item_id))
        .sum()
}

fn withdraw_network_material(
    sim: &mut Simulation,
    network: &super::types::RobotNetworkTopology,
    item_id: ItemId,
) {
    for node in &network.roboports {
        let Some(state) = sim.entities.roboports.get_mut(&node.entity_id) else {
            continue;
        };
        if state.materials.remove(item_id, 1).is_ok() {
            sim.robots.mark_roboport_dirty(node.entity_id);
            return;
        }
    }
    unreachable!("network material availability was checked before dispatch");
}

fn dispatching_roboport(
    sim: &Simulation,
    network: &super::types::RobotNetworkTopology,
    target: (i64, i64),
) -> Option<(EntityId, ItemId, u64)> {
    let mut best: Option<(i128, EntityId, ItemId, u64)> = None;
    for node in &network.roboports {
        let Some(state) = sim.entities.roboports.get(&node.entity_id) else {
            continue;
        };
        let Some(robot_item) = state
            .robots
            .slots()
            .iter()
            .filter_map(|slot| slot.stack())
            .map(|stack| stack.item_id())
            .find(|item_id| {
                sim.world
                    .prototypes
                    .item(*item_id)
                    .and_then(|item| item.robot)
                    .is_some_and(|robot| robot.kind == RobotKind::Construction)
            })
        else {
            continue;
        };
        let profile = sim
            .world
            .prototypes
            .item(robot_item)
            .and_then(|item| item.robot)
            .expect("selected robot has a flight profile");
        if state.charge_energy_joules < profile.energy_capacity_joules {
            continue;
        }
        let Some(dock) = roboport_dock_position(&sim.entities, node.entity_id) else {
            continue;
        };
        let distance = squared_distance(dock.0 - target.0, dock.1 - target.1);
        if best.is_none_or(|(best_distance, best_id, ..)| {
            distance < best_distance || (distance == best_distance && node.entity_id < best_id)
        }) {
            best = Some((
                distance,
                node.entity_id,
                robot_item,
                profile.energy_capacity_joules,
            ));
        }
    }
    best.map(|(_, entity_id, item_id, energy)| (entity_id, item_id, energy))
}
